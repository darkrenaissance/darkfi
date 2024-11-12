/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
    /// new finalized blocks. Upon receiving them, all the transactions are
    /// scanned and we check if any of them call the money contract, and if
    /// the payments are intended for us. If so, we decrypt them and append
    /// the metadata to our wallet.
    pub async fn subscribe_blocks(
        &self,
        endpoint: Url,
        ex: Arc<smol::Executor<'static>>,
    ) -> Result<()> {
        // Grab last known block
        let rep = self
            .darkfid_daemon_request("blockchain.last_known_block", &JsonValue::Array(vec![]))
            .await?;
        let mut last_known = *rep.get::<f64>().unwrap() as u32;

        // Handle genesis(0) block
        if last_known == 0 {
            if let Err(e) = self.scan_blocks(true).await {
                return Err(Error::DatabaseError(format!(
                    "[subscribe_blocks] Scanning from genesis block failed: {e:?}"
                )))
            }
        }

        // Grab last known block again
        let rep = self
            .darkfid_daemon_request("blockchain.last_known_block", &JsonValue::Array(vec![]))
            .await?;
        last_known = *rep.get::<f64>().unwrap() as u32;
        let last_scanned = match self.get_last_scanned_block() {
            Ok((l, _)) => l,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[subscribe_blocks] Retrieving last scanned block failed: {e:?}"
                )))
            }
        };

        // When no other block has been created
        if last_known != last_scanned {
            eprintln!("Warning: Last scanned block is not the last known block.");
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

                        let block_data: BlockInfo = deserialize_async(&bytes).await?;
                        println!("Deserialized successfully. Scanning block...");
                        if let Err(e) = self.scan_block(&block_data).await {
                            return Err(Error::DatabaseError(format!(
                                "[subscribe_blocks] Scanning block failed: {e:?}"
                            )))
                        }
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
            let tx_hash = tx.hash().to_string();
            let mut wallet_tx = false;
            println!("[scan_block] Processing transaction: {tx_hash}");
            for (i, call) in tx.calls.iter().enumerate() {
                if call.data.contract_id == *MONEY_CONTRACT_ID {
                    println!("[scan_block] Found Money contract in call {i}");
                    if self.apply_tx_money_data(i, &tx.calls, &tx_hash).await? {
                        wallet_tx = true;
                    };
                    continue
                }

                if call.data.contract_id == *DAO_CONTRACT_ID {
                    println!("[scan_block] Found DAO contract in call {i}");
                    if self
                        .apply_tx_dao_data(
                            &call.data.data,
                            TransactionHash::new(
                                *blake3::hash(&serialize_async(tx).await).as_bytes(),
                            ),
                            i as u8,
                        )
                        .await?
                    {
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
        if let Err(e) = self.put_tx_history_records(&wallet_txs, "Finalized").await {
            return Err(Error::DatabaseError(format!(
                "[scan_block] Inserting transaction history records failed: {e:?}"
            )))
        }

        // Store this block rollback query
        self.store_inverse_cache(block.header.height, &block.hash().to_string())?;

        Ok(())
    }

    /// Scans the blockchain starting from the last scanned block, for relevant
    /// money transfer transactions. If reset flag is provided, Merkle tree state
    /// and coins are reset, and start scanning from beginning. Alternatively,
    /// it looks for a checkpoint in the wallet to reset and start scanning from.
    pub async fn scan_blocks(&self, reset: bool) -> WalletDbResult<()> {
        // Grab last scanned block height
        let (mut height, _) = self.get_last_scanned_block()?;
        // If last scanned block is genesis (0) or reset flag
        // has been provided we reset, otherwise continue with
        // the next block height
        if height == 0 || reset {
            self.reset_scanned_blocks()?;
            self.reset_money_tree().await?;
            self.reset_money_smt()?;
            self.reset_money_coins()?;
            self.reset_mint_authorities()?;
            self.reset_dao_trees().await?;
            self.reset_daos().await?;
            self.reset_dao_proposals().await?;
            self.reset_dao_votes()?;
            self.reset_tx_history()?;
            height = 0;
        } else {
            height += 1;
        };

        loop {
            let rep = match self
                .darkfid_daemon_request("blockchain.last_known_block", &JsonValue::Array(vec![]))
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[scan_blocks] RPC client request failed: {e:?}");
                    return Err(WalletDbError::GenericError)
                }
            };
            let last = *rep.get::<f64>().unwrap() as u32;

            println!("Requested to scan from block number: {height}");
            println!("Last known block number reported by darkfid: {last}");

            // Already scanned last known block
            if height > last {
                return Ok(())
            }

            while height <= last {
                println!("Requesting block {height}...");
                let block = match self.get_block_by_height(height).await {
                    Ok(r) => r,
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

    /// Queries darkfid for given transaction's gas.
    pub async fn get_tx_gas(&self, tx: &Transaction, include_fee: bool) -> Result<u64> {
        let params = JsonValue::Array(vec![
            JsonValue::String(base64::encode(&serialize_async(tx).await)),
            JsonValue::Boolean(include_fee),
        ]);
        let rep = self.darkfid_daemon_request("tx.calculate_gas", &params).await?;

        let gas = *rep.get::<f64>().unwrap() as u64;

        Ok(gas)
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
