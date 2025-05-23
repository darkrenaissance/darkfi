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

use std::{sync::Arc, time::Instant};

use url::Url;

use darkfi::{
    blockchain::BlockInfo,
    rpc::{
        client::RpcClient,
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult},
        util::JsonValue,
    },
    system::{Publisher, StoppableTask},
    tx::Transaction,
    util::encoding::base64,
    Error, Result,
};
use darkfi_sdk::{
    crypto::{ContractId, DAO_CONTRACT_ID, DEPLOYOOOR_CONTRACT_ID, MONEY_CONTRACT_ID},
    tx::TransactionHash,
};
use darkfi_serial::{deserialize_async, serialize_async};

use crate::{
    error::{WalletDbError, WalletDbResult},
    Drk,
};

impl Drk {
    /// Subscribes to darkfid's JSON-RPC notification endpoint that serves
    /// new confirmed blocks. Upon receiving them, all the transactions are
    /// scanned and we check if any of them call the money contract, and if
    /// the payments are intended for us. If so, we decrypt them and append
    /// the metadata to our wallet. If a reorg block is received, we revert
    /// to its previous height and then scan it. We assume that the blocks
    /// up to that point are unchanged, since darkfid will just broadcast
    /// the sequence after the reorg.
    pub async fn subscribe_blocks(
        &self,
        endpoint: Url,
        ex: Arc<smol::Executor<'static>>,
    ) -> Result<()> {
        // Grab last confirmed block height
        let (last_confirmed_height, _) = self.get_last_confirmed_block().await?;

        // Handle genesis(0) block
        if last_confirmed_height == 0 {
            if let Err(e) = self.scan_blocks().await {
                return Err(Error::DatabaseError(format!(
                    "[subscribe_blocks] Scanning from genesis block failed: {e:?}"
                )))
            }
        }

        // Grab last confirmed block again
        let (last_confirmed_height, last_confirmed_hash) = self.get_last_confirmed_block().await?;

        // Grab last scanned block
        let (mut last_scanned_height, last_scanned_hash) = match self.get_last_scanned_block() {
            Ok(last) => last,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[subscribe_blocks] Retrieving last scanned block failed: {e:?}"
                )))
            }
        };

        // Check if other blocks have been created
        if last_confirmed_height != last_scanned_height || last_confirmed_hash != last_scanned_hash
        {
            eprintln!("Warning: Last scanned block is not the last confirmed block.");
            eprintln!("You should first fully scan the blockchain, and then subscribe");
            return Err(Error::DatabaseError(
                "[subscribe_blocks] Blockchain not fully scanned".to_string(),
            ))
        }

        println!("Subscribing to receive notifications of incoming blocks");
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
            ex,
        );
        println!("Detached subscription to background");
        println!("All is good. Waiting for block notifications...");

        let e = loop {
            match subscription.receive().await {
                JsonResult::Notification(n) => {
                    println!("Got Block notification from darkfid subscription");
                    if n.method != "blockchain.subscribe_blocks" {
                        break Error::UnexpectedJsonRpc(format!(
                            "Got foreign notification from darkfid: {}",
                            n.method
                        ))
                    }

                    // Verify parameters
                    if !n.params.is_array() {
                        break Error::UnexpectedJsonRpc(
                            "Received notification params are not an array".to_string(),
                        )
                    }
                    let params = n.params.get::<Vec<JsonValue>>().unwrap();
                    if params.is_empty() {
                        break Error::UnexpectedJsonRpc(
                            "Notification parameters are empty".to_string(),
                        )
                    }

                    for param in params {
                        let param = param.get::<String>().unwrap();
                        let bytes = base64::decode(param).unwrap();

                        let block: BlockInfo = deserialize_async(&bytes).await?;
                        println!("Deserialized successfully. Scanning block...");

                        // Check if a reorg block was received, to reset to its previous
                        if block.header.height <= last_scanned_height {
                            let reset_height = block.header.height.saturating_sub(1);
                            if let Err(e) = self.reset_to_height(reset_height).await {
                                return Err(Error::DatabaseError(format!(
                                    "[subscribe_blocks] Wallet state reset failed: {e:?}"
                                )))
                            }

                            // Scan genesis again if needed
                            if reset_height == 0 {
                                let genesis = match self.get_block_by_height(reset_height).await {
                                    Ok(b) => b,
                                    Err(e) => {
                                        return Err(Error::Custom(format!(
                                            "[subscribe_blocks] RPC client request failed: {e:?}"
                                        )))
                                    }
                                };
                                if let Err(e) = self.scan_block(&genesis).await {
                                    return Err(Error::DatabaseError(format!(
                                        "[subscribe_blocks] Scanning block failed: {e:?}"
                                    )))
                                };
                            }
                        }

                        if let Err(e) = self.scan_block(&block).await {
                            return Err(Error::DatabaseError(format!(
                                "[subscribe_blocks] Scanning block failed: {e:?}"
                            )))
                        }

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
                    break Error::UnexpectedJsonRpc(format!(
                        "Got unexpected data from JSON-RPC: {x:?}"
                    ))
                }
            }
        };

        Err(e)
    }

    /// `scan_block` will go over over transactions in a block and handle their calls
    /// based on the called contract. Additionally, will update `last_scanned_block` to
    /// the provided block height and will store its height, hash and inverse query.
    async fn scan_block(&self, block: &BlockInfo) -> Result<()> {
        // Reset wallet inverse cache state
        self.reset_inverse_cache().await?;

        // Keep track of our wallet transactions
        let mut wallet_txs = vec![];
        println!("=======================================");
        println!("{}", block.header);
        println!("=======================================");
        println!("[scan_block] Iterating over {} transactions", block.txs.len());
        for tx in block.txs.iter() {
            let tx_hash = tx.hash();
            let tx_hash_string = tx_hash.to_string();
            let mut wallet_tx = false;
            println!("[scan_block] Processing transaction: {tx_hash_string}");
            for (i, call) in tx.calls.iter().enumerate() {
                if call.data.contract_id == *MONEY_CONTRACT_ID {
                    println!("[scan_block] Found Money contract in call {i}");
                    if self.apply_tx_money_data(i, &tx.calls, &tx_hash_string).await? {
                        wallet_tx = true;
                    };
                    continue
                }

                if call.data.contract_id == *DAO_CONTRACT_ID {
                    println!("[scan_block] Found DAO contract in call {i}");
                    if self.apply_tx_dao_data(&call.data.data, tx_hash, i as u8).await? {
                        wallet_tx = true;
                    };
                    continue
                }

                if call.data.contract_id == *DEPLOYOOOR_CONTRACT_ID {
                    println!("[scan_block] Found DeployoOor contract in call {i}");
                    // TODO: implement
                    continue
                }

                // TODO: For now we skip non-native contract calls
                println!("[scan_block] Found non-native contract in call {i}, skipping.");
            }

            // If this is our wallet tx we mark it for update
            if wallet_tx {
                wallet_txs.push(tx);
            }
        }

        // Update wallet transactions records
        if let Err(e) = self.put_tx_history_records(&wallet_txs, "Confirmed").await {
            return Err(Error::DatabaseError(format!(
                "[scan_block] Inserting transaction history records failed: {e:?}"
            )))
        }

        // Store this block rollback query
        self.store_inverse_cache(block.header.height, &block.hash().to_string())?;

        Ok(())
    }

    /// Scans the blockchain for wallet relevant transactions,
    /// starting from the last scanned block. If a reorg has happened,
    /// we revert to its previous height and then scan from there.
    pub async fn scan_blocks(&self) -> WalletDbResult<()> {
        // Grab last scanned block height
        let (mut height, hash) = self.get_last_scanned_block()?;

        // Grab our last scanned block from darkfid
        let block = match self.get_block_by_height(height).await {
            Ok(b) => Some(b),
            // Check if block was found
            Err(Error::JsonRpcError((-32121, _))) => None,
            Err(e) => {
                eprintln!("[scan_blocks] RPC client request failed: {e:?}");
                return Err(WalletDbError::GenericError)
            }
        };

        // Check if a reorg has happened
        if block.is_none() || hash != block.unwrap().hash().to_string() {
            // Find the exact block height the reorg happened
            println!("A reorg has happened, finding last known common block...");
            height = height.saturating_sub(1);
            while height != 0 {
                // Grab our scanned block hash for that height
                let (_, scanned_block_hash, _) = self.get_scanned_block_record(height)?;

                // Grab the block from darkfid for that height
                let block = match self.get_block_by_height(height).await {
                    Ok(b) => Some(b),
                    // Check if block was found
                    Err(Error::JsonRpcError((-32121, _))) => None,
                    Err(e) => {
                        eprintln!("[scan_blocks] RPC client request failed: {e:?}");
                        return Err(WalletDbError::GenericError)
                    }
                };

                // Continue to previous one if they don't match
                if block.is_none() || scanned_block_hash != block.unwrap().hash().to_string() {
                    height = height.saturating_sub(1);
                    continue
                }

                // Reset to its height
                println!("Last common block found: {height} - {scanned_block_hash}");
                self.reset_to_height(height).await?;
                break
            }
        }

        // If last scanned block is genesis(0) we reset,
        // otherwise continue with the next block height.
        if height == 0 {
            self.reset().await?;
        } else {
            height += 1;
        }

        loop {
            // Grab last confirmed block
            println!("Requested to scan from block number: {height}");
            let (last_height, last_hash) = match self.get_last_confirmed_block().await {
                Ok(last) => last,
                Err(e) => {
                    eprintln!("[scan_blocks] RPC client request failed: {e:?}");
                    return Err(WalletDbError::GenericError)
                }
            };
            println!("Last confirmed block reported by darkfid: {last_height} - {last_hash}");

            // Already scanned last confirmed block
            if height > last_height {
                return Ok(())
            }

            while height <= last_height {
                println!("Requesting block {height}...");
                let block = match self.get_block_by_height(height).await {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("[scan_blocks] RPC client request failed: {e:?}");
                        return Err(WalletDbError::GenericError)
                    }
                };
                println!("Block {height} received! Scanning block...");
                if let Err(e) = self.scan_block(&block).await {
                    eprintln!("[scan_blocks] Scan block failed: {e:?}");
                    return Err(WalletDbError::GenericError)
                };
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
    pub async fn broadcast_tx(&self, tx: &Transaction) -> Result<String> {
        println!("Broadcasting transaction...");

        let params =
            JsonValue::Array(vec![JsonValue::String(base64::encode(&serialize_async(tx).await))]);
        let rep = self.darkfid_daemon_request("tx.broadcast", &params).await?;

        let txid = rep.get::<String>().unwrap().clone();

        // Store transactions history record
        if let Err(e) = self.put_tx_history_record(tx, "Broadcasted").await {
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
    pub async fn ping(&self) -> Result<()> {
        println!("Executing ping request to darkfid...");
        let latency = Instant::now();
        let rep = self.darkfid_daemon_request("ping", &JsonValue::Array(vec![])).await?;
        let latency = latency.elapsed();
        println!("Got reply: {rep:?}");
        println!("Latency: {latency:?}");
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
