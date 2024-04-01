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
use darkfi_sdk::{crypto::ContractId, tx::TransactionHash};
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
        let rep = self.rpc_client.request(req).await?;
        let last_known = *rep.get::<f64>().unwrap() as u64;
        let last_scanned = match self.last_scanned_block().await {
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
                        if let Err(e) = self.scan_block_money(&block_data).await {
                            return Err(Error::RusqliteError(format!(
                                "[subscribe_blocks] Scaning blocks for Money failed: {e:?}"
                            )))
                        }
                        self.scan_block_dao(&block_data).await?;
                        if let Err(e) = self
                            .update_tx_history_records_status(&block_data.txs, "Finalized")
                            .await
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

    /// `scan_block_money` will go over transactions in a block and fetch the ones dealing
    /// with the money contract. Then over all of them, try to see if any are related
    /// to us. If any are found, the metadata is extracted and placed into the wallet
    /// for future use.
    async fn scan_block_money(&self, block: &BlockInfo) -> Result<()> {
        println!("[Money] Iterating over {} transactions", block.txs.len());

        for tx in block.txs.iter() {
            self.apply_tx_money_data(tx, true).await?;
        }

        // Write this block height into `last_scanned_block`
        let query =
            format!("UPDATE {} SET {} = ?1;", *MONEY_INFO_TABLE, MONEY_INFO_COL_LAST_SCANNED_BLOCK);
        if let Err(e) = self.wallet.exec_sql(&query, rusqlite::params![block.header.height]).await {
            return Err(Error::RusqliteError(format!(
                "[scan_block_money] Update last scanned block failed: {e:?}"
            )))
        }

        Ok(())
    }

    /// `scan_block_dao` will go over transactions in a block and fetch the ones dealing
    /// with the dao contract. Then over all of them, try to see if any are related
    /// to us. If any are found, the metadata is extracted and placed into the wallet
    /// for future use.
    async fn scan_block_dao(&self, block: &BlockInfo) -> Result<()> {
        println!("[DAO] Iterating over {} transactions", block.txs.len());
        for tx in block.txs.iter() {
            self.apply_tx_dao_data(tx, true).await?;
        }

        Ok(())
    }

    /// Scans the blockchain starting from the last scanned block, for relevant
    /// money transfer transactions. If reset flag is provided, Merkle tree state
    /// and coins are reset, and start scanning from beginning. Alternatively,
    /// it looks for a checkpoint in the wallet to reset and start scanning from.
    pub async fn scan_blocks(&self, reset: bool) -> WalletDbResult<()> {
        // Grab last scanned block height
        let mut height = self.last_scanned_block().await?;
        // If last scanned block is genesis (0) or reset flag
        // has been provided we reset, otherwise continue with
        // the next block height
        if height == 0 || reset {
            self.reset_money_tree().await?;
            self.reset_money_coins().await?;
            self.reset_dao_trees().await?;
            self.reset_daos().await?;
            self.reset_dao_proposals().await?;
            self.reset_dao_votes().await?;
            self.update_all_tx_history_records_status("Rejected").await?;
            height = 0;
        } else {
            height += 1;
        };

        loop {
            let req = JsonRequest::new("blockchain.last_known_block", JsonValue::Array(vec![]));
            let rep = match self.rpc_client.request(req).await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[scan_blocks] RPC client request failed: {e:?}");
                    return Err(WalletDbError::GenericError)
                }
            };
            let last = *rep.get::<f64>().unwrap() as u64;

            println!("Requested to scan from block number: {height}");
            println!("Last known block number reported by darkfid: {last}");

            // Already scanned last known block
            if height >= last {
                return Ok(())
            }

            while height <= last {
                eprint!("Requesting block {}... ", height);
                let block = match self.get_block_by_height(height).await {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("[scan_blocks] RPC client request failed: {e:?}");
                        return Err(WalletDbError::GenericError)
                    }
                };
                if let Err(e) = self.scan_block_money(&block).await {
                    eprintln!("[scan_blocks] Scan block Money failed: {e:?}");
                    return Err(WalletDbError::GenericError)
                };
                if let Err(e) = self.scan_block_dao(&block).await {
                    eprintln!("[scan_blocks] Scan block DAO failed: {e:?}");
                    return Err(WalletDbError::GenericError)
                };
                self.update_tx_history_records_status(&block.txs, "Finalized").await?;
                height += 1;
            }
        }
    }

    // Queries darkfid for a block with given height
    async fn get_block_by_height(&self, height: u64) -> Result<BlockInfo> {
        let req = JsonRequest::new(
            "blockchain.get_block",
            JsonValue::Array(vec![JsonValue::String(height.to_string())]),
        );

        let params = self.rpc_client.request(req).await?;
        let param = params.get::<String>().unwrap();
        let bytes = base64::decode(param).unwrap();
        let block = deserialize_async(&bytes).await?;
        Ok(block)
    }

    /// Broadcast a given transaction to darkfid and forward onto the network.
    /// Returns the transaction ID upon success
    pub async fn broadcast_tx(&self, tx: &Transaction) -> Result<String> {
        println!("Broadcasting transaction...");

        let params =
            JsonValue::Array(vec![JsonValue::String(base64::encode(&serialize_async(tx).await))]);
        let req = JsonRequest::new("tx.broadcast", params);
        let rep = self.rpc_client.request(req).await?;

        let txid = rep.get::<String>().unwrap().clone();

        // Store transactions history record
        if let Err(e) = self.insert_tx_history_record(tx).await {
            return Err(Error::RusqliteError(format!(
                "[broadcast_tx] Inserting transaction history record failed: {e:?}"
            )))
        }

        Ok(txid)
    }

    /// Queries darkfid for a tx with given hash
    pub async fn get_tx(&self, tx_hash: &TransactionHash) -> Result<Option<Transaction>> {
        let tx_hash_str = tx_hash.to_string();
        let req = JsonRequest::new(
            "blockchain.get_tx",
            JsonValue::Array(vec![JsonValue::String(tx_hash_str)]),
        );

        match self.rpc_client.request(req).await {
            Ok(param) => {
                let tx_bytes = base64::decode(param.get::<String>().unwrap()).unwrap();
                let tx = deserialize_async(&tx_bytes).await?;
                Ok(Some(tx))
            }

            Err(_) => Ok(None),
        }
    }

    /// Simulate the transaction with the state machine
    pub async fn simulate_tx(&self, tx: &Transaction) -> Result<bool> {
        let tx_str = base64::encode(&serialize_async(tx).await);
        let req =
            JsonRequest::new("tx.simulate", JsonValue::Array(vec![JsonValue::String(tx_str)]));
        let rep = self.rpc_client.request(req).await?;

        let is_valid = *rep.get::<bool>().unwrap();
        Ok(is_valid)
    }

    /// Try to fetch zkas bincodes for the given `ContractId`.
    pub async fn lookup_zkas(&self, contract_id: &ContractId) -> Result<Vec<(String, Vec<u8>)>> {
        println!("Querying zkas bincode for {contract_id}");

        let params = JsonValue::Array(vec![JsonValue::String(format!("{contract_id}"))]);
        let req = JsonRequest::new("blockchain.lookup_zkas", params);

        let rep = self.rpc_client.request(req).await?;
        let params = rep.get::<Vec<JsonValue>>().unwrap();

        let mut ret = Vec::with_capacity(params.len());
        for param in params {
            let zkas_ns = param[0].get::<String>().unwrap().clone();
            let zkas_bincode_bytes = base64::decode(param.get::<String>().unwrap()).unwrap();
            let zkas_bincode = deserialize_async(&zkas_bincode_bytes).await?;
            ret.push((zkas_ns, zkas_bincode));
        }

        Ok(ret)
    }
}
