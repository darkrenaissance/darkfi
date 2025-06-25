/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{
    collections::{BTreeMap, HashMap},
    time::Instant,
};

use smol::channel::Sender;
use url::Url;

use darkfi::{
    blockchain::BlockInfo,
    rpc::{
        client::RpcClient,
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult},
        util::JsonValue,
    },
    system::{ExecutorPtr, Publisher, StoppableTask},
    tx::Transaction,
    util::encoding::base64,
    Error, Result,
};
use darkfi_dao_contract::model::{DaoBulla, DaoProposalBulla};
use darkfi_money_contract::model::TokenId;
use darkfi_sdk::{
    bridgetree::Position,
    crypto::{
        smt::{PoseidonFp, EMPTY_NODES_FP},
        ContractId, MerkleTree, SecretKey, DAO_CONTRACT_ID, DEPLOYOOOR_CONTRACT_ID,
        MONEY_CONTRACT_ID,
    },
    tx::TransactionHash,
};
use darkfi_serial::{deserialize_async, serialize_async};

use crate::{
    cache::{CacheOverlay, CacheSmt, CacheSmtStorage, SLED_MONEY_SMT_TREE},
    cli_util::append_or_print,
    dao::{SLED_MERKLE_TREES_DAO_DAOS, SLED_MERKLE_TREES_DAO_PROPOSALS},
    error::{WalletDbError, WalletDbResult},
    money::SLED_MERKLE_TREES_MONEY,
    Drk, DrkPtr,
};

/// Auxiliary structure holding various in memory caches to use during scan
pub struct ScanCache {
    /// The Money Merkle tree containing coins
    pub money_tree: MerkleTree,
    /// The Money Sparse Merkle tree containing coins nullifiers
    pub money_smt: CacheSmt,
    /// All our known secrets to decrypt coin notes
    pub notes_secrets: Vec<SecretKey>,
    /// Our own coins nullifiers and their leaf positions
    pub owncoins_nullifiers: BTreeMap<[u8; 32], ([u8; 32], Position)>,
    /// Our own tokens to track freezes
    pub own_tokens: Vec<TokenId>,
    /// The DAO Merkle tree containing DAO bullas
    pub dao_daos_tree: MerkleTree,
    /// The DAO Merkle tree containing proposals bullas
    pub dao_proposals_tree: MerkleTree,
    /// Our own DAOs with their proposals and votes keys
    pub own_daos: HashMap<DaoBulla, (Option<SecretKey>, Option<SecretKey>)>,
    /// Our own DAOs proposals with their corresponding DAO reference
    pub own_proposals: HashMap<DaoProposalBulla, DaoBulla>,
    /// Messages buffer for better downstream prints handling
    pub messages_buffer: Vec<String>,
}

impl ScanCache {
    /// Auxiliary function to append messages to the buffer.
    pub fn log(&mut self, msg: String) {
        self.messages_buffer.push(msg);
    }

    /// Auxiliary function to consume the messages buffer.
    pub fn flush_messages(&mut self) -> Vec<String> {
        self.messages_buffer.drain(..).collect()
    }
}

impl Drk {
    /// Auxiliary function to generate a new [`ScanCache`] for the
    /// wallet.
    pub async fn scan_cache(&self) -> Result<ScanCache> {
        let money_tree = self.get_money_tree().await?;
        let smt_store = CacheSmtStorage::new(CacheOverlay::new(&self.cache)?, SLED_MONEY_SMT_TREE);
        let money_smt = CacheSmt::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
        let mut notes_secrets = self.get_money_secrets().await?;
        let mut owncoins_nullifiers = BTreeMap::new();
        for coin in self.get_coins(true).await? {
            owncoins_nullifiers.insert(
                coin.0.nullifier().to_bytes(),
                (coin.0.coin.to_bytes(), coin.0.leaf_position),
            );
        }
        let mint_authorities = self.get_mint_authorities().await?;
        let mut own_tokens = Vec::with_capacity(mint_authorities.len());
        for (token, _, _, _, _) in mint_authorities {
            own_tokens.push(token);
        }
        let (dao_daos_tree, dao_proposals_tree) = self.get_dao_trees().await?;
        let mut own_daos = HashMap::new();
        for dao in self.get_daos().await? {
            own_daos.insert(
                dao.bulla(),
                (dao.params.proposals_secret_key, dao.params.votes_secret_key),
            );
            if let Some(secret_key) = dao.params.notes_secret_key {
                notes_secrets.push(secret_key);
            }
        }
        let mut own_proposals = HashMap::new();
        for proposal in self.get_proposals().await? {
            own_proposals.insert(proposal.bulla(), proposal.proposal.dao_bulla);
        }

        Ok(ScanCache {
            money_tree,
            money_smt,
            notes_secrets,
            owncoins_nullifiers,
            own_tokens,
            dao_daos_tree,
            dao_proposals_tree,
            own_daos,
            own_proposals,
            messages_buffer: vec![],
        })
    }

    /// `scan_block` will go over over transactions in a block and handle their calls
    /// based on the called contract.
    async fn scan_block(&self, scan_cache: &mut ScanCache, block: &BlockInfo) -> Result<()> {
        // Keep track of the trees we need to update and our wallet
        // transactions.
        let mut update_money_tree = false;
        let mut update_dao_daos_tree = false;
        let mut update_dao_proposals_tree = false;
        let mut wallet_txs = vec![];

        // Scan the block
        scan_cache.log(String::from("======================================="));
        scan_cache.log(format!("{}", block.header));
        scan_cache.log(String::from("======================================="));
        scan_cache.log(format!("[scan_block] Iterating over {} transactions", block.txs.len()));
        for tx in block.txs.iter() {
            let tx_hash = tx.hash();
            let tx_hash_string = tx_hash.to_string();
            let mut wallet_tx = false;
            scan_cache.log(format!("[scan_block] Processing transaction: {tx_hash_string}"));
            for (i, call) in tx.calls.iter().enumerate() {
                if call.data.contract_id == *MONEY_CONTRACT_ID {
                    scan_cache.log(format!("[scan_block] Found Money contract in call {i}"));
                    let (update_tree, own_tx) = self
                        .apply_tx_money_data(
                            scan_cache,
                            &i,
                            &tx.calls,
                            &tx_hash_string,
                            &block.header.height,
                        )
                        .await?;
                    if update_tree {
                        update_money_tree = true;
                    }
                    if own_tx {
                        wallet_tx = true;
                    }
                    continue
                }

                if call.data.contract_id == *DAO_CONTRACT_ID {
                    scan_cache.log(format!("[scan_block] Found DAO contract in call {i}"));
                    let (update_daos_tree, update_proposals_tree, own_tx) = self
                        .apply_tx_dao_data(
                            scan_cache,
                            &call.data.data,
                            &tx_hash,
                            &(i as u8),
                            &block.header.height,
                        )
                        .await?;
                    if update_daos_tree {
                        update_dao_daos_tree = true;
                    }
                    if update_proposals_tree {
                        update_dao_proposals_tree = true;
                    }
                    if own_tx {
                        wallet_tx = true;
                    }
                    continue
                }

                if call.data.contract_id == *DEPLOYOOOR_CONTRACT_ID {
                    scan_cache.log(format!("[scan_block] Found DeployoOor contract in call {i}"));
                    // TODO: implement
                    continue
                }

                // TODO: For now we skip non-native contract calls
                scan_cache
                    .log(format!("[scan_block] Found non-native contract in call {i}, skipping."));
            }

            // If this is our wallet tx we mark it for update
            if wallet_tx {
                wallet_txs.push(tx);
            }
        }

        // Update money merkle tree, if needed
        if update_money_tree {
            scan_cache
                .money_smt
                .store
                .overlay
                .insert_merkle_tree(SLED_MERKLE_TREES_MONEY, &scan_cache.money_tree)?;
        }

        // Update dao daos merkle tree, if needed
        if update_dao_daos_tree {
            scan_cache
                .money_smt
                .store
                .overlay
                .insert_merkle_tree(SLED_MERKLE_TREES_DAO_DAOS, &scan_cache.dao_daos_tree)?;
        }

        // Update dao proposals merkle tree, if needed
        if update_dao_proposals_tree {
            scan_cache.money_smt.store.overlay.insert_merkle_tree(
                SLED_MERKLE_TREES_DAO_PROPOSALS,
                &scan_cache.dao_proposals_tree,
            )?;
        }

        // Insert the block record
        scan_cache
            .money_smt
            .store
            .overlay
            .insert_scanned_block(&block.header.height, &block.header.hash())?;

        // Grab the overlay current diff
        let diff = scan_cache.money_smt.store.overlay.0.diff(&[])?;

        // Apply the overlay current changes
        scan_cache.money_smt.store.overlay.0.apply_diff(&diff)?;

        // Insert the state inverse diff record
        self.cache.insert_state_inverse_diff(&block.header.height, &diff.inverse())?;

        // Flush sled
        self.cache.sled_db.flush()?;

        // Update wallet transactions records
        if let Err(e) =
            self.put_tx_history_records(&wallet_txs, "Confirmed", Some(block.header.height)).await
        {
            return Err(Error::DatabaseError(format!(
                "[scan_block] Inserting transaction history records failed: {e:?}"
            )))
        }

        Ok(())
    }

    /// Scans the blockchain for wallet relevant transactions,
    /// starting from the last scanned block. If a reorg has happened,
    /// we revert to its previous height and then scan from there.
    pub async fn scan_blocks(
        &self,
        output: &mut Vec<String>,
        sender: Option<&Sender<Vec<String>>>,
        print: &bool,
    ) -> WalletDbResult<()> {
        // Grab last scanned block height
        let (mut height, hash) = self.get_last_scanned_block()?;

        // Grab our last scanned block from darkfid
        let block = match self.get_block_by_height(height).await {
            Ok(b) => Some(b),
            // Check if block was found
            Err(Error::JsonRpcError((-32121, _))) => None,
            Err(e) => {
                append_or_print(
                    output,
                    sender,
                    print,
                    vec![format!("[scan_blocks] RPC client request failed: {e:?}")],
                )
                .await;
                return Err(WalletDbError::GenericError)
            }
        };

        // Check if a reorg has happened
        if block.is_none() || hash != block.unwrap().hash().to_string() {
            // Find the exact block height the reorg happened
            let mut buf =
                vec![String::from("A reorg has happened, finding last known common block...")];
            height = height.saturating_sub(1);
            while height != 0 {
                // Grab our scanned block hash for that height
                let scanned_block_hash = self.get_scanned_block_hash(&height)?;

                // Grab the block from darkfid for that height
                let block = match self.get_block_by_height(height).await {
                    Ok(b) => Some(b),
                    // Check if block was found
                    Err(Error::JsonRpcError((-32121, _))) => None,
                    Err(e) => {
                        buf.push(format!("[scan_blocks] RPC client request failed: {e:?}"));
                        append_or_print(output, sender, print, buf).await;
                        return Err(WalletDbError::GenericError)
                    }
                };

                // Continue to previous one if they don't match
                if block.is_none() || scanned_block_hash != block.unwrap().hash().to_string() {
                    height = height.saturating_sub(1);
                    continue
                }

                // Reset to its height
                buf.push(format!("Last common block found: {height} - {scanned_block_hash}"));
                self.reset_to_height(height, &mut buf)?;
                append_or_print(output, sender, print, buf).await;
                break
            }
        }

        // If last scanned block is genesis(0) we reset,
        // otherwise continue with the next block height.
        if height == 0 {
            let mut buf = vec![];
            self.reset(&mut buf)?;
            append_or_print(output, sender, print, buf).await;
        } else {
            height += 1;
        }

        // Generate a new scan cache
        let mut scan_cache = match self.scan_cache().await {
            Ok(c) => c,
            Err(e) => {
                append_or_print(
                    output,
                    sender,
                    print,
                    vec![format!("[scan_blocks] Generating scan cache failed: {e:?}")],
                )
                .await;
                return Err(WalletDbError::GenericError)
            }
        };

        loop {
            // Grab last confirmed block
            let mut buf = vec![format!("Requested to scan from block number: {height}")];
            let (last_height, last_hash) = match self.get_last_confirmed_block().await {
                Ok(last) => last,
                Err(e) => {
                    buf.push(format!("[scan_blocks] RPC client request failed: {e:?}"));
                    append_or_print(output, sender, print, buf).await;
                    return Err(WalletDbError::GenericError)
                }
            };
            buf.push(format!(
                "Last confirmed block reported by darkfid: {last_height} - {last_hash}"
            ));
            append_or_print(output, sender, print, buf).await;

            // Already scanned last confirmed block
            if height > last_height {
                return Ok(())
            }

            while height <= last_height {
                let mut buf = vec![format!("Requesting block {height}...")];
                buf.push(format!("Requesting block {height}..."));
                let block = match self.get_block_by_height(height).await {
                    Ok(b) => b,
                    Err(e) => {
                        buf.push(format!("[scan_blocks] RPC client request failed: {e:?}"));
                        append_or_print(output, sender, print, buf).await;
                        return Err(WalletDbError::GenericError)
                    }
                };
                buf.push(format!("Block {height} received! Scanning block..."));
                if let Err(e) = self.scan_block(&mut scan_cache, &block).await {
                    buf.push(format!("[scan_blocks] Scan block failed: {e:?}"));
                    append_or_print(output, sender, print, buf).await;
                    return Err(WalletDbError::GenericError)
                };
                for msg in scan_cache.flush_messages() {
                    buf.push(msg);
                }
                append_or_print(output, sender, print, buf).await;
                height += 1;
            }
        }
    }

    // Queries darkfid for last confirmed block.
    async fn get_last_confirmed_block(&self) -> Result<(u32, String)> {
        let rep = self
            .darkfid_daemon_request("blockchain.last_confirmed_block", &JsonValue::Array(vec![]))
            .await?;
        let params = rep.get::<Vec<JsonValue>>().unwrap();
        let height = *params[0].get::<f64>().unwrap() as u32;
        let hash = params[1].get::<String>().unwrap().clone();

        Ok((height, hash))
    }

    // Queries darkfid for a block with given height.
    async fn get_block_by_height(&self, height: u32) -> Result<BlockInfo> {
        let params = self
            .darkfid_daemon_request(
                "blockchain.get_block",
                &JsonValue::Array(vec![JsonValue::String(height.to_string())]),
            )
            .await?;
        let param = params.get::<String>().unwrap();
        let bytes = base64::decode(param).unwrap();
        let block = deserialize_async(&bytes).await?;
        Ok(block)
    }

    /// Broadcast a given transaction to darkfid and forward onto the network.
    /// Returns the transaction ID upon success.
    pub async fn broadcast_tx(&self, tx: &Transaction, output: &mut Vec<String>) -> Result<String> {
        output.push(String::from("Broadcasting transaction..."));

        let params =
            JsonValue::Array(vec![JsonValue::String(base64::encode(&serialize_async(tx).await))]);
        let rep = self.darkfid_daemon_request("tx.broadcast", &params).await?;

        let txid = rep.get::<String>().unwrap().clone();

        // Store transactions history record
        if let Err(e) = self.put_tx_history_record(tx, "Broadcasted", None).await {
            return Err(Error::DatabaseError(format!(
                "[broadcast_tx] Inserting transaction history record failed: {e:?}"
            )))
        }

        Ok(txid)
    }

    /// Queries darkfid for a tx with given hash.
    pub async fn get_tx(&self, tx_hash: &TransactionHash) -> Result<Option<Transaction>> {
        let tx_hash_str = tx_hash.to_string();
        match self
            .darkfid_daemon_request(
                "blockchain.get_tx",
                &JsonValue::Array(vec![JsonValue::String(tx_hash_str)]),
            )
            .await
        {
            Ok(param) => {
                let tx_bytes = base64::decode(param.get::<String>().unwrap()).unwrap();
                let tx = deserialize_async(&tx_bytes).await?;
                Ok(Some(tx))
            }

            Err(_) => Ok(None),
        }
    }

    /// Simulate the transaction with the state machine.
    pub async fn simulate_tx(&self, tx: &Transaction) -> Result<bool> {
        let tx_str = base64::encode(&serialize_async(tx).await);
        let rep = self
            .darkfid_daemon_request(
                "tx.simulate",
                &JsonValue::Array(vec![JsonValue::String(tx_str)]),
            )
            .await?;

        let is_valid = *rep.get::<bool>().unwrap();
        Ok(is_valid)
    }

    /// Try to fetch zkas bincodes for the given `ContractId`.
    pub async fn lookup_zkas(&self, contract_id: &ContractId) -> Result<Vec<(String, Vec<u8>)>> {
        let params = JsonValue::Array(vec![JsonValue::String(format!("{contract_id}"))]);
        let rep = self.darkfid_daemon_request("blockchain.lookup_zkas", &params).await?;
        let params = rep.get::<Vec<JsonValue>>().unwrap();

        let mut ret = Vec::with_capacity(params.len());
        for param in params {
            let zkas_ns = param[0].get::<String>().unwrap().clone();
            let zkas_bincode_bytes = base64::decode(param[1].get::<String>().unwrap()).unwrap();
            ret.push((zkas_ns, zkas_bincode_bytes));
        }

        Ok(ret)
    }

    /// Queries darkfid for given transaction's required fee.
    pub async fn get_tx_fee(&self, tx: &Transaction, include_fee: bool) -> Result<u64> {
        let params = JsonValue::Array(vec![
            JsonValue::String(base64::encode(&serialize_async(tx).await)),
            JsonValue::Boolean(include_fee),
        ]);
        let rep = self.darkfid_daemon_request("tx.calculate_fee", &params).await?;

        let fee = *rep.get::<f64>().unwrap() as u64;

        Ok(fee)
    }

    /// Queries darkfid for current best fork next height.
    pub async fn get_next_block_height(&self) -> Result<u32> {
        let rep = self
            .darkfid_daemon_request(
                "blockchain.best_fork_next_block_height",
                &JsonValue::Array(vec![]),
            )
            .await?;

        let next_height = *rep.get::<f64>().unwrap() as u32;

        Ok(next_height)
    }

    /// Queries darkfid for currently configured block target time.
    pub async fn get_block_target(&self) -> Result<u32> {
        let rep = self
            .darkfid_daemon_request("blockchain.block_target", &JsonValue::Array(vec![]))
            .await?;

        let next_height = *rep.get::<f64>().unwrap() as u32;

        Ok(next_height)
    }

    /// Auxiliary function to ping configured darkfid daemon for liveness.
    pub async fn ping(&self, output: &mut Vec<String>) -> Result<()> {
        output.push(String::from("Executing ping request to darkfid..."));
        let latency = Instant::now();
        let rep = self.darkfid_daemon_request("ping", &JsonValue::Array(vec![])).await?;
        let latency = latency.elapsed();
        output.push(format!("Got reply: {rep:?}"));
        output.push(format!("Latency: {latency:?}"));
        Ok(())
    }

    /// Auxiliary function to execute a request towards the configured darkfid daemon JSON-RPC endpoint.
    pub async fn darkfid_daemon_request(
        &self,
        method: &str,
        params: &JsonValue,
    ) -> Result<JsonValue> {
        let Some(ref rpc_client) = self.rpc_client else { return Err(Error::RpcClientStopped) };
        let req = JsonRequest::new(method, params.clone());
        let rep = rpc_client.request(req).await?;
        Ok(rep)
    }

    /// Auxiliary function to stop current JSON-RPC client, if its initialized.
    pub async fn stop_rpc_client(&self) -> Result<()> {
        if let Some(ref rpc_client) = self.rpc_client {
            rpc_client.stop().await;
        };
        Ok(())
    }
}

/// Subscribes to darkfid's JSON-RPC notification endpoint that serves
/// new confirmed blocks. Upon receiving them, all the transactions are
/// scanned and we check if any of them call the money contract, and if
/// the payments are intended for us. If so, we decrypt them and append
/// the metadata to our wallet. If a reorg block is received, we revert
/// to its previous height and then scan it. We assume that the blocks
/// up to that point are unchanged, since darkfid will just broadcast
/// the sequence after the reorg.
pub async fn subscribe_blocks(
    drk: &DrkPtr,
    shell_sender: Sender<Vec<String>>,
    endpoint: Url,
    ex: &ExecutorPtr,
) -> Result<()> {
    // First we do a clean scan
    let lock = drk.read().await;
    if let Err(e) = lock.scan_blocks(&mut vec![], Some(&shell_sender), &false).await {
        let err_msg = format!("Failed during scanning: {e:?}");
        shell_sender.send(vec![err_msg.clone()]).await?;
        return Err(Error::Custom(err_msg))
    }
    shell_sender.send(vec![String::from("Finished scanning blockchain")]).await?;

    // Grab last confirmed block height
    let (last_confirmed_height, _) = lock.get_last_confirmed_block().await?;

    // Handle genesis(0) block
    if last_confirmed_height == 0 {
        if let Err(e) = lock.scan_blocks(&mut vec![], Some(&shell_sender), &false).await {
            let err_msg = format!("[subscribe_blocks] Scanning from genesis block failed: {e:?}");
            shell_sender.send(vec![err_msg.clone()]).await?;
            return Err(Error::Custom(err_msg))
        }
    }

    // Grab last confirmed block again
    let (last_confirmed_height, last_confirmed_hash) = lock.get_last_confirmed_block().await?;

    // Grab last scanned block
    let (mut last_scanned_height, last_scanned_hash) = match lock.get_last_scanned_block() {
        Ok(last) => last,
        Err(e) => {
            let err_msg = format!("[subscribe_blocks] Retrieving last scanned block failed: {e:?}");
            shell_sender.send(vec![err_msg.clone()]).await?;
            return Err(Error::Custom(err_msg))
        }
    };

    // Check if other blocks have been created
    if last_confirmed_height != last_scanned_height || last_confirmed_hash != last_scanned_hash {
        let err_msg = String::from("[subscribe_blocks] Blockchain not fully scanned");
        shell_sender
            .send(vec![
                String::from("Warning: Last scanned block is not the last confirmed block."),
                String::from("You should first fully scan the blockchain, and then subscribe"),
                err_msg.clone(),
            ])
            .await?;
        return Err(Error::Custom(err_msg))
    }

    let mut shell_message =
        vec![String::from("Subscribing to receive notifications of incoming blocks")];
    let publisher = Publisher::new();
    let subscription = publisher.clone().subscribe().await;
    let _publisher = publisher.clone();
    let _ex = ex.clone();
    StoppableTask::new().start(
        // Weird hack to prevent lifetimes hell
        async move {
            let rpc_client = RpcClient::new(endpoint, _ex).await?;
            let req = JsonRequest::new("blockchain.subscribe_blocks", JsonValue::Array(vec![]));
            rpc_client.subscribe(req, _publisher).await
        },
        |res| async move {
            match res {
                Ok(()) => { /* Do nothing */ }
                Err(e) => {
                    eprintln!("[subscribe_blocks] JSON-RPC server error: {e:?}");
                    publisher
                        .notify(JsonResult::Error(JsonError::new(
                            ErrorCode::InternalError,
                            None,
                            0,
                        )))
                        .await;
                }
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );
    shell_message.push(String::from("Detached subscription to background"));
    shell_message.push(String::from("All is good. Waiting for block notifications..."));
    shell_sender.send(shell_message).await?;
    drop(lock);

    let e = 'outer: loop {
        match subscription.receive().await {
            JsonResult::Notification(n) => {
                let mut shell_message =
                    vec![String::from("Got Block notification from darkfid subscription")];
                if n.method != "blockchain.subscribe_blocks" {
                    shell_sender.send(shell_message).await?;
                    break Error::UnexpectedJsonRpc(format!(
                        "Got foreign notification from darkfid: {}",
                        n.method
                    ))
                }

                // Verify parameters
                if !n.params.is_array() {
                    shell_sender.send(shell_message).await?;
                    break Error::UnexpectedJsonRpc(
                        "Received notification params are not an array".to_string(),
                    )
                }
                let params = n.params.get::<Vec<JsonValue>>().unwrap();
                if params.is_empty() {
                    shell_sender.send(shell_message).await?;
                    break Error::UnexpectedJsonRpc("Notification parameters are empty".to_string())
                }

                for param in params {
                    let param = param.get::<String>().unwrap();
                    let bytes = base64::decode(param).unwrap();

                    let block: BlockInfo = deserialize_async(&bytes).await?;
                    shell_message
                        .push(String::from("Deserialized successfully. Scanning block..."));

                    // Check if a reorg block was received, to reset to its previous
                    let lock = drk.read().await;
                    if block.header.height <= last_scanned_height {
                        let reset_height = block.header.height.saturating_sub(1);
                        if let Err(e) = lock.reset_to_height(reset_height, &mut shell_message) {
                            shell_sender.send(shell_message).await?;
                            break 'outer Error::Custom(format!(
                                "[subscribe_blocks] Wallet state reset failed: {e:?}"
                            ))
                        }

                        // Scan genesis again if needed
                        if reset_height == 0 {
                            let genesis = match lock.get_block_by_height(reset_height).await {
                                Ok(b) => b,
                                Err(e) => {
                                    shell_sender.send(shell_message).await?;
                                    break 'outer Error::Custom(format!(
                                        "[subscribe_blocks] RPC client request failed: {e:?}"
                                    ))
                                }
                            };
                            let mut scan_cache = lock.scan_cache().await?;
                            if let Err(e) = lock.scan_block(&mut scan_cache, &genesis).await {
                                shell_sender.send(shell_message).await?;
                                break 'outer Error::Custom(format!(
                                    "[subscribe_blocks] Scanning block failed: {e:?}"
                                ))
                            };
                            for msg in scan_cache.flush_messages() {
                                shell_message.push(msg);
                            }
                        }
                    }

                    let mut scan_cache = lock.scan_cache().await?;
                    if let Err(e) = lock.scan_block(&mut scan_cache, &block).await {
                        shell_sender.send(shell_message).await?;
                        break 'outer Error::Custom(format!(
                            "[subscribe_blocks] Scanning block failed: {e:?}"
                        ))
                    }
                    for msg in scan_cache.flush_messages() {
                        shell_message.push(msg);
                    }
                    shell_sender.send(shell_message.clone()).await?;

                    // Set new last scanned block height
                    last_scanned_height = block.header.height;
                }
            }

            JsonResult::Error(e) => {
                // Some error happened in the transmission
                break Error::UnexpectedJsonRpc(format!("Got error from JSON-RPC: {e:?}"))
            }

            x => {
                // And this is weird
                break Error::UnexpectedJsonRpc(format!("Got unexpected data from JSON-RPC: {x:?}"))
            }
        }
    };

    shell_sender.send(vec![format!("[subscribe_blocks] Subscription loop break: {e}")]).await?;
    Err(e)
}
