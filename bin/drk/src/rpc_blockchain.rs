/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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
use darkfi_money_contract::{
    client::{
        Coin, EncryptedNote, OwnCoin, MONEY_COINS_COL_COIN, MONEY_COINS_COL_COIN_BLIND,
        MONEY_COINS_COL_IS_SPENT, MONEY_COINS_COL_LEAF_POSITION, MONEY_COINS_COL_MEMO,
        MONEY_COINS_COL_NULLIFIER, MONEY_COINS_COL_SECRET, MONEY_COINS_COL_SERIAL,
        MONEY_COINS_COL_TOKEN_BLIND, MONEY_COINS_COL_TOKEN_ID, MONEY_COINS_COL_VALUE,
        MONEY_COINS_COL_VALUE_BLIND, MONEY_COINS_TABLE, MONEY_INFO_COL_LAST_SCANNED_SLOT,
        MONEY_INFO_TABLE,
    },
    state::{MoneyTransferParams, Output},
    MoneyFunction,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, ContractId, MerkleNode, Nullifier},
    incrementalmerkletree::Tree,
    pasta::pallas,
};
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
        eprintln!("Subscribing to receive notifications of incoming blocks");
        let subscriber = Subscriber::new();
        let subscription = subscriber.clone().subscribe().await;

        let rpc_client = RpcClient::new(endpoint).await?;

        let req = JsonRequest::new("blockchain.subscribe_blocks", json!([]));
        task::spawn(async move { rpc_client.subscribe(req, subscriber).await.unwrap() });
        eprintln!("Detached subscription to background");

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

                    // TODO: FIXME: Disallow this if last_scanned_slot is not this-1 or something
                    eprintln!("Deserialized successfully. Scanning block...");
                    self.scan_block(&block_data).await?;
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

    /// `scan_block` will go over transactions in a block and fetch the ones dealing
    /// with the money contract. Then over all of them, try to see if any are related
    /// to us. If any are found, the metadata is extracted and placed into the wallet
    /// for future use.
    async fn scan_block(&self, block: &BlockInfo) -> Result<()> {
        eprintln!("Iterating over {} transactions", block.txs.len());

        let mut outputs: Vec<Output> = vec![];

        // TODO: FIXME: This shouldn't be hardcoded here obviously.
        let contract_id = ContractId::from(pallas::Base::from(u64::MAX - 420));

        for (i, tx) in block.txs.iter().enumerate() {
            for (j, call) in tx.calls.iter().enumerate() {
                if call.contract_id == contract_id && call.data[0] == MoneyFunction::Transfer as u8
                {
                    eprintln!("Found Money::Transfer in call {} in tx {}", j, i);
                    let params: MoneyTransferParams = deserialize(&call.data[1..])?;
                    for output in params.outputs {
                        outputs.push(output);
                    }
                    continue
                }

                if call.contract_id == contract_id && call.data[0] == MoneyFunction::OtcSwap as u8 {
                    eprintln!("Found Money::OtcSwap in call {} in tx {}", j, i);
                    let params: MoneyTransferParams = deserialize(&call.data[1..])?;
                    for output in params.outputs {
                        outputs.push(output);
                    }
                    continue
                }
            }
        }

        // Fetch our secret keys from the wallet
        eprintln!("Fetching secret keys from wallet");
        let secrets = self.wallet_secrets().await?;
        if secrets.is_empty() {
            eprintln!("Warning: No secrets found in wallet");
        }

        eprintln!("Fetching Merkle tree from wallet");
        let mut tree = self.wallet_tree().await?;

        let mut owncoins = vec![];

        // FIXME: We end up adding duplicate coins that could already be in the tree
        for output in outputs {
            let coin = output.coin;

            // Append the new coin to the Merkle tree. Every coin has to be added.
            tree.append(&MerkleNode::from(coin));

            // Attempt to decrypt the note
            let enc_note =
                EncryptedNote { ciphertext: output.ciphertext, ephem_public: output.ephem_public };

            for secret in &secrets {
                if let Ok(note) = enc_note.decrypt(secret) {
                    eprintln!("Successfully decrypted a note");
                    eprintln!("Witnessing coin in Merkle tree");
                    let leaf_position = tree.witness().unwrap();

                    let owncoin = OwnCoin {
                        coin: Coin::from(coin),
                        note: note.clone(),
                        secret: *secret,
                        nullifier: Nullifier::from(poseidon_hash([secret.inner(), note.serial])),
                        leaf_position,
                    };

                    owncoins.push(owncoin);
                }
            }
        }

        eprintln!("Serializing the Merkle tree into the wallet");
        self.put_tree(&tree).await?;
        eprintln!("Merkle tree written successfully");

        // This is the SQL query we'll be executing to insert coins into the wallet
        let query = format!(
            "INSERT INTO {} ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12);",
            MONEY_COINS_TABLE,
            MONEY_COINS_COL_COIN,
            MONEY_COINS_COL_IS_SPENT,
            MONEY_COINS_COL_SERIAL,
            MONEY_COINS_COL_VALUE,
            MONEY_COINS_COL_TOKEN_ID,
            MONEY_COINS_COL_COIN_BLIND,
            MONEY_COINS_COL_VALUE_BLIND,
            MONEY_COINS_COL_TOKEN_BLIND,
            MONEY_COINS_COL_SECRET,
            MONEY_COINS_COL_NULLIFIER,
            MONEY_COINS_COL_LEAF_POSITION,
            MONEY_COINS_COL_MEMO,
        );

        eprintln!("Found {} OwnCoin(s) in block", owncoins.len());
        for owncoin in owncoins {
            eprintln!("Owncoin: {:?}", owncoin.coin);
            let params = json!([
                query,
                QueryType::Blob as u8,
                serialize(&owncoin.coin),
                QueryType::Integer as u8,
                0, // <-- is_spent
                QueryType::Blob as u8,
                serialize(&owncoin.note.serial),
                QueryType::Blob as u8,
                serialize(&owncoin.note.value),
                QueryType::Blob as u8,
                serialize(&owncoin.note.token_id),
                QueryType::Blob as u8,
                serialize(&owncoin.note.coin_blind),
                QueryType::Blob as u8,
                serialize(&owncoin.note.value_blind),
                QueryType::Blob as u8,
                serialize(&owncoin.note.token_blind),
                QueryType::Blob as u8,
                serialize(&owncoin.secret),
                QueryType::Blob as u8,
                serialize(&owncoin.nullifier),
                QueryType::Blob as u8,
                serialize(&owncoin.leaf_position),
                QueryType::Blob as u8,
                serialize(&owncoin.note.memo),
            ]);

            eprintln!("Executing JSON-RPC request to add OwnCoin to wallet");
            let req = JsonRequest::new("wallet.exec_sql", params);
            self.rpc_client.request(req).await?;
            eprintln!("Coin added successfully");
        }

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
        Ok(txid)
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

    /// Scans the blockchain optionally starting from the given slot for relevant
    /// money transfer transactions. Alternatively it looks for a checkpoint in the
    /// wallet to start scanning from.
    pub async fn scan_blocks(&self, slot: Option<u64>) -> Result<()> {
        let mut sl = if let Some(sl) = slot { sl } else { self.wallet_last_scanned_slot().await? };

        let req = JsonRequest::new("blockchain.last_known_slot", json!([]));
        let rep = self.rpc_client.request(req).await?;

        let last: u64 = serde_json::from_value(rep)?;

        eprintln!("Requested to scan from slot number: {}", sl);
        eprintln!("Last known slot number reported by darkfid: {}", last);

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
            if sl > last {
                term_tx.close();
                break
            }

            sl += 1;

            eprint!("Requesting slot {}... ", sl);
            if let Some(block) = self.get_block_by_slot(sl).await? {
                eprintln!("Found");
                self.scan_block(&block).await?;
            } else {
                eprintln!("Not found");
            }

            // Write down the slot number into back to the wallet
            let query = format!(
                "UPDATE {} SET {} = ?1;",
                MONEY_INFO_TABLE, MONEY_INFO_COL_LAST_SCANNED_SLOT
            );
            let params = json!([query, QueryType::Integer as u8, sl]);
            let req = JsonRequest::new("wallet.exec_sql", params);
            let _ = self.rpc_client.request(req).await?;
        }

        handle.close();
        signals_task.await;

        Ok(())
    }
}
