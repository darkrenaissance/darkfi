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
    Error, Result,
};
use darkfi_money_contract::client::{MONEY_INFO_COL_LAST_SCANNED_SLOT, MONEY_INFO_TABLE};
use darkfi_serial::{deserialize, serialize};

use super::{
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
        let req = JsonRequest::new("blockchain.last_known_slot", JsonValue::Array(vec![]));
        let rep = self.rpc_client.request(req).await?;
        let last_known = *rep.get::<f64>().unwrap() as u64;
        let last_scanned = match self.last_scanned_slot().await {
            Ok(l) => l,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[subscribe_blocks] Retrieving last scanned slot failed: {e:?}"
                )))
            }
        };

        if last_known != last_scanned {
            eprintln!("Warning: Last scanned slot is not the last known slot.");
            eprintln!("You should first fully scan the blockchain, and then subscribe");
            return Err(Error::RusqliteError(
                "[subscribe_blocks] Blockchain not fully scanned".to_string(),
            ))
        }

        eprintln!("Subscribing to receive notifications of incoming blocks");
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
        eprintln!("Detached subscription to background");
        eprintln!("All is good. Waiting for block notifications...");

        let e = loop {
            match subscription.receive().await {
                JsonResult::Notification(n) => {
                    eprintln!("Got Block notification from darkfid subscription");
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
                        let bytes = bs58::decode(param).into_vec()?;

                        let block_data: BlockInfo = deserialize(&bytes)?;
                        eprintln!("=======================================");
                        eprintln!("Block header:\n{:#?}", block_data.header);
                        eprintln!("=======================================");

                        eprintln!("Deserialized successfully. Scanning block...");
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
        eprintln!("[Money] Iterating over {} transactions", block.txs.len());

        for tx in block.txs.iter() {
            self.apply_tx_money_data(tx, true).await?;
        }

        // Write this slot into `last_scanned_slot`
        let query =
            format!("UPDATE {} SET {} = ?1;", MONEY_INFO_TABLE, MONEY_INFO_COL_LAST_SCANNED_SLOT);
        if let Err(e) = self.wallet.exec_sql(&query, rusqlite::params![block.header.height]).await {
            return Err(Error::RusqliteError(format!(
                "[scan_block_money] Update last scanned slot failed: {e:?}"
            )))
        }

        Ok(())
    }

    /// `scan_block_dao` will go over transactions in a block and fetch the ones dealing
    /// with the dao contract. Then over all of them, try to see if any are related
    /// to us. If any are found, the metadata is extracted and placed into the wallet
    /// for future use.
    async fn scan_block_dao(&self, block: &BlockInfo) -> Result<()> {
        eprintln!("[DAO] Iterating over {} transactions", block.txs.len());
        for tx in block.txs.iter() {
            self.apply_tx_dao_data(tx, true).await?;
        }

        Ok(())
    }

    /// Scans the blockchain starting from the last scanned slot, for relevant
    /// money transfer transactions. If reset flag is provided, Merkle tree state
    /// and coins are reset, and start scanning from beginning. Alternatively,
    /// it looks for a checkpoint in the wallet to reset and start scanning from.
    pub async fn scan_blocks(&self, reset: bool) -> WalletDbResult<()> {
        let mut sl = if reset {
            self.reset_money_tree().await?;
            self.reset_money_coins().await?;
            self.reset_dao_trees().await?;
            self.reset_daos().await?;
            self.reset_dao_proposals().await?;
            self.reset_dao_votes().await?;
            self.update_all_tx_history_records_status("Rejected").await?;
            0
        } else {
            self.last_scanned_slot().await?
        };

        let req = JsonRequest::new("blockchain.last_known_slot", JsonValue::Array(vec![]));
        let rep = match self.rpc_client.request(req).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[scan_blocks] RPC client request failed: {e:?}");
                return Err(WalletDbError::GenericError)
            }
        };
        let last = *rep.get::<f64>().unwrap() as u64;

        eprintln!("Requested to scan from slot number: {sl}");
        eprintln!("Last known slot number reported by darkfid: {last}");

        // Already scanned last known slot
        if sl == last {
            return Ok(())
        }

        while sl <= last {
            eprint!("Requesting slot {}... ", sl);
            let requested_block = match self.get_block_by_slot(sl).await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[scan_blocks] RPC client request failed: {e:?}");
                    return Err(WalletDbError::GenericError)
                }
            };
            if let Some(block) = requested_block {
                eprintln!("Found");
                if let Err(e) = self.scan_block_money(&block).await {
                    eprintln!("[scan_blocks] Scan block Money failed: {e:?}");
                    return Err(WalletDbError::GenericError)
                };
                if let Err(e) = self.scan_block_dao(&block).await {
                    eprintln!("[scan_blocks] Scan block DAO failed: {e:?}");
                    return Err(WalletDbError::GenericError)
                };
                self.update_tx_history_records_status(&block.txs, "Finalized").await?;
            } else {
                eprintln!("Not found");
                // Write down the slot number into back to the wallet
                // This might be a bit intense, but we accept it for now.
                let query = format!(
                    "UPDATE {} SET {} = ?1;",
                    MONEY_INFO_TABLE, MONEY_INFO_COL_LAST_SCANNED_SLOT
                );
                self.wallet.exec_sql(&query, rusqlite::params![sl]).await?;
            }
            sl += 1;
        }

        Ok(())
    }

    // Queries darkfid for a block with given slot
    async fn get_block_by_slot(&self, slot: u64) -> Result<Option<BlockInfo>> {
        let req = JsonRequest::new(
            "blockchain.get_slot",
            JsonValue::Array(vec![JsonValue::String(slot.to_string())]),
        );

        // This API is weird, we need some way of telling it's an empty slot and
        // not an error
        match self.rpc_client.request(req).await {
            Ok(params) => {
                let param = params.get::<String>().unwrap();
                let bytes = bs58::decode(param).into_vec()?;
                let block = deserialize(&bytes)?;
                Ok(Some(block))
            }

            Err(_) => Ok(None),
        }
    }

    /// Broadcast a given transaction to darkfid and forward onto the network.
    /// Returns the transaction ID upon success
    pub async fn broadcast_tx(&self, tx: &Transaction) -> Result<String> {
        eprintln!("Broadcasting transaction...");

        let params =
            JsonValue::Array(vec![JsonValue::String(bs58::encode(&serialize(tx)).into_string())]);
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
}
