/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use anyhow::{anyhow, Result};
use async_std::{stream::StreamExt, task};
use darkfi::{
    consensus::BlockInfo,
    rpc::{
        client::RpcClient,
        jsonrpc::{JsonRequest, JsonResult},
    },
    system::Subscriber,
    tx::Transaction,
    wallet::walletdb::QueryType,
};
use darkfi_money_contract::client::{MONEY_INFO_COL_LAST_SCANNED_SLOT, MONEY_INFO_TABLE};
use darkfi_sdk::crypto::ContractId;
use darkfi_serial::{deserialize, serialize};
use serde_json::json;
use signal_hook::consts::{SIGINT, SIGQUIT, SIGTERM};
use signal_hook_async_std::Signals;
use url::Url;

use super::Drk;

impl Drk {
    /// Subscribes to darkfid's JSON-RPC notification endpoint that serves
    /// new finalized blocks. Upon receiving them, all the transactions are
    /// scanned and we check if any of them call the money contract, and if
    /// the payments are intended for us. If so, we decrypt them and append
    /// the metadata to our wallet.
    pub async fn subscribe_blocks(&self, endpoint: Url) -> Result<()> {
        let req = JsonRequest::new("blockchain.last_known_slot", json!([]));
        let rep = self.rpc_client.request(req).await?;
        let last_known: u64 = serde_json::from_value(rep)?;
        let last_scanned = self.last_scanned_slot().await?;

        if last_known != last_scanned {
            eprintln!("Warning: Last scanned slot is not the last known slot.");
            eprintln!("You should first fully scan the blockchain, and then subscribe");
            return Err(anyhow!("Blockchain not fully scanned"))
        }

        eprintln!("Subscribing to receive notifications of incoming blocks");
        let subscriber = Subscriber::new();
        let subscription = subscriber.clone().subscribe().await;

        let rpc_client = RpcClient::new(endpoint).await?;

        let req = JsonRequest::new("blockchain.subscribe_blocks", json!([]));
        task::spawn(async move { rpc_client.subscribe(req, subscriber).await.unwrap() });
        eprintln!("Detached subscription to background");
        eprintln!("All is good. Waiting for block notifications...");

        let e = loop {
            match subscription.receive().await {
                JsonResult::Notification(n) => {
                    eprintln!("Got Block notification from darkfid subscription");
                    if n.method != "blockchain.subscribe_blocks" {
                        break anyhow!("Got foreign notification from darkfid: {}", n.method)
                    }

                    let Some(params) = n.params.as_array() else {
                        break anyhow!("Received notification params are not an array")
                    };

                    if params.len() != 1 {
                        break anyhow!("Notification parameters are not len 1")
                    }

                    let params = n.params.as_array().unwrap()[0].as_str().unwrap();
                    let bytes = bs58::decode(params).into_vec()?;

                    let block_data: BlockInfo = deserialize(&bytes)?;
                    eprintln!("=======================================");
                    eprintln!("Block header:\n{:#?}", block_data.header);
                    eprintln!("=======================================");

                    eprintln!("Deserialized successfully. Scanning block...");
                    self.scan_block_money(&block_data).await?;
                    self.scan_block_dao(&block_data).await?;
                    self.update_tx_history_records_status(&block_data.txs, "Finalized").await?;
                }

                JsonResult::Error(e) => {
                    // Some error happened in the transmission
                    break anyhow!("Got error from JSON-RPC: {:?}", e)
                }

                x => {
                    // And this is weird
                    break anyhow!("Got unexpected data from JSON-RPC: {:?}", x)
                }
            }
        };

        Err(e)
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
        let params = json!([query, QueryType::Integer as u8, block.header.slot]);
        let req = JsonRequest::new("wallet.exec_sql", params);
        let _ = self.rpc_client.request(req).await?;

        Ok(())
    }

    /// Try to fetch zkas bincodes for the given `ContractId`.
    pub async fn lookup_zkas(&self, contract_id: &ContractId) -> Result<Vec<(String, Vec<u8>)>> {
        eprintln!("Querying zkas bincode for {}", contract_id);

        let params = json!([format!("{}", contract_id)]);
        let req = JsonRequest::new("blockchain.lookup_zkas", params);

        let rep = self.rpc_client.request(req).await?;

        let ret = serde_json::from_value(rep)?;
        Ok(ret)
    }

    /// Broadcast a given transaction to darkfid and forward onto the network.
    /// Returns the transaction ID upon success
    pub async fn broadcast_tx(&self, tx: &Transaction) -> Result<String> {
        eprintln!("Broadcasting transaction...");

        let params = json!([bs58::encode(&serialize(tx)).into_string()]);
        let req = JsonRequest::new("tx.broadcast", params);
        let rep = self.rpc_client.request(req).await?;

        let txid = serde_json::from_value(rep)?;

        // Store transactions history record
        self.insert_tx_history_record(tx).await?;

        Ok(txid)
    }

    /// Simulate the transaction with the state machine
    pub async fn simulate_tx(&self, tx: &Transaction) -> Result<bool> {
        let params = json!([bs58::encode(&serialize(tx)).into_string()]);
        let req = JsonRequest::new("tx.simulate", params);
        let rep = self.rpc_client.request(req).await?;

        let is_valid = serde_json::from_value(rep)?;
        Ok(is_valid)
    }

    /// Queries darkfid for a block with given slot
    async fn get_block_by_slot(&self, slot: u64) -> Result<Option<BlockInfo>> {
        let req = JsonRequest::new("blockchain.get_slot", json!([slot]));

        // This API is weird, we need some way of telling it's an empty slot and
        // not an error
        match self.rpc_client.request(req).await {
            Ok(v) => {
                let block_bytes: Vec<u8> = serde_json::from_value(v)?;
                let block = deserialize(&block_bytes)?;
                Ok(Some(block))
            }

            Err(_) => Ok(None),
        }
    }

    /// Queries darkfid for a tx with given hash
    pub async fn get_tx(&self, tx_hash: &blake3::Hash) -> Result<Option<Transaction>> {
        let tx_hash_str: &str = &tx_hash.to_hex();
        let req = JsonRequest::new("blockchain.get_tx", json!([tx_hash_str]));

        match self.rpc_client.request(req).await {
            Ok(v) => {
                let tx_bytes: Vec<u8> = serde_json::from_value(v)?;
                let tx = deserialize(&tx_bytes)?;
                Ok(Some(tx))
            }

            Err(_) => Ok(None),
        }
    }

    /// Scans the blockchain starting from the last scanned slot, for relevant
    /// money transfer transactions. If reset flag is provided, Merkle tree state
    /// and coins are reset, and start scanning from beginning. Alternatively,
    /// it looks for a checkpoint in the wallet to reset and start scanning from.
    pub async fn scan_blocks(&self, reset: bool) -> Result<()> {
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

        let req = JsonRequest::new("blockchain.last_known_slot", json!([]));
        let rep = self.rpc_client.request(req).await?;
        let last: u64 = serde_json::from_value(rep)?;

        eprintln!("Requested to scan from slot number: {}", sl);
        eprintln!("Last known slot number reported by darkfid: {}", last);

        // Already scanned last known slot
        if sl == last {
            return Ok(())
        }

        // We set this up to handle an interrupt
        let mut signals = Signals::new([SIGTERM, SIGINT, SIGQUIT])?;
        let handle = signals.handle();
        let (term_tx, _term_rx) = smol::channel::bounded::<()>(1);

        let term_tx_ = term_tx.clone();
        let signals_task = task::spawn(async move {
            while let Some(signal) = signals.next().await {
                match signal {
                    SIGTERM | SIGINT | SIGQUIT => term_tx_.close(),
                    _ => unreachable!(),
                };
            }
        });

        while !term_tx.is_closed() {
            sl += 1;

            if sl > last {
                term_tx.close();
                break
            }

            eprint!("Requesting slot {}... ", sl);
            if let Some(block) = self.get_block_by_slot(sl).await? {
                eprintln!("Found");
                self.scan_block_money(&block).await?;
                self.scan_block_dao(&block).await?;
                self.update_tx_history_records_status(&block.txs, "Finalized").await?;
            } else {
                eprintln!("Not found");
                // Write down the slot number into back to the wallet
                // This might be a bit intense, but we accept it for now.
                let query = format!(
                    "UPDATE {} SET {} = ?1;",
                    MONEY_INFO_TABLE, MONEY_INFO_COL_LAST_SCANNED_SLOT
                );
                let params = json!([query, QueryType::Integer as u8, sl]);
                let req = JsonRequest::new("wallet.exec_sql", params);
                let _ = self.rpc_client.request(req).await?;
            }
        }

        handle.close();
        signals_task.await;

        Ok(())
    }

    /// Subscribes to darkfid's JSON-RPC notification endpoint that serves
    /// erroneous transactions rejections.
    pub async fn subscribe_err_txs(&self, endpoint: Url) -> Result<()> {
        eprintln!("Subscribing to receive notifications of erroneous transactions");
        let subscriber = Subscriber::new();
        let subscription = subscriber.clone().subscribe().await;

        let rpc_client = RpcClient::new(endpoint).await?;

        let req = JsonRequest::new("blockchain.subscribe_err_txs", json!([]));
        task::spawn(async move { rpc_client.subscribe(req, subscriber).await.unwrap() });
        eprintln!("Detached subscription to background");
        eprintln!("All is good. Waiting for erroneous transactions notifications...");

        let e = loop {
            match subscription.receive().await {
                JsonResult::Notification(n) => {
                    eprintln!("Got erroneous transaction notification from darkfid subscription");
                    if n.method != "blockchain.subscribe_err_txs" {
                        break anyhow!("Got foreign notification from darkfid: {}", n.method)
                    }

                    let Some(params) = n.params.as_array() else {
                        break anyhow!("Received notification params are not an array")
                    };

                    if params.len() != 1 {
                        break anyhow!("Notification parameters are not len 1")
                    }

                    let params = n.params.as_array().unwrap()[0].as_str().unwrap();
                    let bytes = bs58::decode(params).into_vec()?;

                    let tx_hash: String = deserialize(&bytes)?;
                    eprintln!("===================================");
                    eprintln!("Erroneous transaction: {}", tx_hash);
                    eprintln!("===================================");
                    self.update_tx_history_record_status(&tx_hash, "Rejected").await?;
                }

                JsonResult::Error(e) => {
                    // Some error happened in the transmission
                    break anyhow!("Got error from JSON-RPC: {:?}", e)
                }

                x => {
                    // And this is weird
                    break anyhow!("Got unexpected data from JSON-RPC: {:?}", x)
                }
            }
        };

        Err(e)
    }
}
