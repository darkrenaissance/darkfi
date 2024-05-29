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

use std::sync::Arc;

use url::Url;

use darkfi::{
    blockchain::BlockInfo,
    rpc::{
        client::RpcClient,
        jsonrpc::{JsonRequest, JsonResult},
        util::JsonValue,
    },
    system::{StoppableTask, Subscriber},
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
    money::{MONEY_INFO_COL_LAST_SCANNED_BLOCK, MONEY_INFO_TABLE},
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
        let req = JsonRequest::new("blockchain.last_known_block", JsonValue::Array(vec![]));
        let rep = self.rpc_client.as_ref().unwrap().request(req).await?;
        let last_known = *rep.get::<f64>().unwrap() as u32;
        let last_scanned = match self.last_scanned_block() {
            Ok(l) => l,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[subscribe_blocks] Retrieving last scanned block failed: {e:?}"
                )))
            }
        };

        if last_known != last_scanned {
            eprintln!("Warning: Last scanned block is not the last known block.");
            eprintln!("You should first fully scan the blockchain, and then subscribe");
            return Err(Error::RusqliteError(
                "[subscribe_blocks] Blockchain not fully scanned".to_string(),
            ))
        }

        println!("Subscribing to receive notifications of incoming blocks");
        let subscriber = Subscriber::new();
        let subscription = subscriber.clone().subscribe().await;
        let _ex = ex.clone();
        StoppableTask::new().start(
            // Weird hack to prevent lifetimes hell
            async move {
                let ex = _ex.clone();
                let rpc_client = RpcClient::new(endpoint, ex).await?;
                let req = JsonRequest::new("blockchain.subscribe_blocks", JsonValue::Array(vec![]));
                rpc_client.subscribe(req, subscriber).await
            },
            |res| async move {
                match res {
                    Ok(()) => {
                        eprintln!("wtf");
                    }
                    Err(e) => eprintln!("[subscribe_blocks] JSON-RPC server error: {e:?}"),
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
                        println!("=======================================");
                        println!("Block header:\n{:#?}", block_data.header);
                        println!("=======================================");

                        println!("Deserialized successfully. Scanning block...");
                        if let Err(e) = self.scan_block(&block_data).await {
                            return Err(Error::RusqliteError(format!(
                                "[subscribe_blocks] Scanning block failed: {e:?}"
                            )))
                        }
                        let txs_hashes = match self.insert_tx_history_records(&block_data.txs).await {
                            Ok(hashes) => hashes,
                            Err(e) => {
                                return Err(Error::RusqliteError(format!(
                                    "[subscribe_blocks] Inserting transaction history records failed: {e:?}"
                                )))
                            },
                        };
                        if let Err(e) =
                            self.update_tx_history_records_status(&txs_hashes, "Finalized")
                        {
                            return Err(Error::RusqliteError(format!(
                                "[subscribe_blocks] Update transaction history record status failed: {e:?}"
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
    /// the probided block height.
    async fn scan_block(&self, block: &BlockInfo) -> Result<()> {
        println!("[scan_block] Iterating over {} transactions", block.txs.len());
        for tx in block.txs.iter() {
            let tx_hash = tx.hash().to_string();
            println!("[scan_block] Processing transaction: {tx_hash}");
            for (i, call) in tx.calls.iter().enumerate() {
                if call.data.contract_id == *MONEY_CONTRACT_ID {
                    println!("[scan_block] Found Money contract in call {i}");
                    self.apply_tx_money_data(i, &tx.calls, &tx_hash).await?;
                    continue
                }

                if call.data.contract_id == *DAO_CONTRACT_ID {
                    println!("[scan_block] Found DAO contract in call {i}");
                    self.apply_tx_dao_data(
                        &call.data.data,
                        TransactionHash::new(*blake3::hash(&serialize_async(tx).await).as_bytes()),
                        i as u8,
                        true,
                    )
                    .await?;
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
        }

        // Write this block height into `last_scanned_block`
        let query =
            format!("UPDATE {} SET {} = ?1;", *MONEY_INFO_TABLE, MONEY_INFO_COL_LAST_SCANNED_BLOCK);
        if let Err(e) = self.wallet.exec_sql(&query, rusqlite::params![block.header.height]) {
            return Err(Error::RusqliteError(format!(
                "[scan_block] Update last scanned block failed: {e:?}"
            )))
        }

        Ok(())
    }

    /// Scans the blockchain starting from the last scanned block, for relevant
    /// money transfer transactions. If reset flag is provided, Merkle tree state
    /// and coins are reset, and start scanning from beginning. Alternatively,
    /// it looks for a checkpoint in the wallet to reset and start scanning from.
    pub async fn scan_blocks(&self, reset: bool) -> WalletDbResult<()> {
        // Grab last scanned block height
        let mut height = self.last_scanned_block()?;
        // If last scanned block is genesis (0) or reset flag
        // has been provided we reset, otherwise continue with
        // the next block height
        if height == 0 || reset {
            self.reset_money_tree().await?;
            self.reset_money_smt()?;
            self.reset_money_coins()?;
            self.reset_dao_trees().await?;
            self.reset_daos().await?;
            self.reset_dao_proposals().await?;
            self.reset_dao_votes()?;
            self.update_all_tx_history_records_status("Rejected")?;
            height = 0;
        } else {
            height += 1;
        };

        loop {
            let req = JsonRequest::new("blockchain.last_known_block", JsonValue::Array(vec![]));
            let rep = match self.rpc_client.as_ref().unwrap().request(req).await {
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
            if height >= last {
                return Ok(())
            }

            while height <= last {
                println!("Requesting block {}... ", height);
                let block = match self.get_block_by_height(height).await {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("[scan_blocks] RPC client request failed: {e:?}");
                        return Err(WalletDbError::GenericError)
                    }
                };
                if let Err(e) = self.scan_block(&block).await {
                    eprintln!("[scan_blocks] Scan block failed: {e:?}");
                    return Err(WalletDbError::GenericError)
                };
                let txs_hashes = self.insert_tx_history_records(&block.txs).await?;
                self.update_tx_history_records_status(&txs_hashes, "Finalized")?;
                height += 1;
            }
        }
    }

    // Queries darkfid for a block with given height.
    async fn get_block_by_height(&self, height: u32) -> Result<BlockInfo> {
        let req = JsonRequest::new(
            "blockchain.get_block",
            JsonValue::Array(vec![JsonValue::String(height.to_string())]),
        );

        let params = self.rpc_client.as_ref().unwrap().request(req).await?;
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
        let req = JsonRequest::new("tx.broadcast", params);
        let rep = self.rpc_client.as_ref().unwrap().request(req).await?;

        let txid = rep.get::<String>().unwrap().clone();

        // Store transactions history record
        if let Err(e) = self.insert_tx_history_record(tx).await {
            return Err(Error::RusqliteError(format!(
                "[broadcast_tx] Inserting transaction history record failed: {e:?}"
            )))
        }

        Ok(txid)
    }

    /// Queries darkfid for a tx with given hash.
    pub async fn get_tx(&self, tx_hash: &TransactionHash) -> Result<Option<Transaction>> {
        let tx_hash_str = tx_hash.to_string();
        let req = JsonRequest::new(
            "blockchain.get_tx",
            JsonValue::Array(vec![JsonValue::String(tx_hash_str)]),
        );

        match self.rpc_client.as_ref().unwrap().request(req).await {
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
        let req =
            JsonRequest::new("tx.simulate", JsonValue::Array(vec![JsonValue::String(tx_str)]));
        let rep = self.rpc_client.as_ref().unwrap().request(req).await?;

        let is_valid = *rep.get::<bool>().unwrap();
        Ok(is_valid)
    }

    /// Try to fetch zkas bincodes for the given `ContractId`.
    pub async fn lookup_zkas(&self, contract_id: &ContractId) -> Result<Vec<(String, Vec<u8>)>> {
        let params = JsonValue::Array(vec![JsonValue::String(format!("{contract_id}"))]);
        let req = JsonRequest::new("blockchain.lookup_zkas", params);

        let rep = self.rpc_client.as_ref().unwrap().request(req).await?;
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
        let req = JsonRequest::new("tx.calculate_gas", params);
        let rep = self.rpc_client.as_ref().unwrap().request(req).await?;

        let gas = *rep.get::<f64>().unwrap() as u64;

        Ok(gas)
    }

    /// Queries darkfid for current best fork next height.
    pub async fn get_next_block_height(&self) -> Result<u32> {
        let req =
            JsonRequest::new("blockchain.best_fork_next_block_height", JsonValue::Array(vec![]));
        let rep = self.rpc_client.as_ref().unwrap().request(req).await?;

        let next_height = *rep.get::<f64>().unwrap() as u32;

        Ok(next_height)
    }
}
