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

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use darkfi::{rpc::jsonrpc::JsonRequest, util::parse::encode_base10, wallet::walletdb::QueryType};
use darkfi_money_contract::client::{
    Coin, Note, OwnCoin, MONEY_COINS_COL_COIN, MONEY_COINS_COL_COIN_BLIND,
    MONEY_COINS_COL_IS_SPENT, MONEY_COINS_COL_LEAF_POSITION, MONEY_COINS_COL_MEMO,
    MONEY_COINS_COL_NULLIFIER, MONEY_COINS_COL_SECRET, MONEY_COINS_COL_SERIAL,
    MONEY_COINS_COL_TOKEN_BLIND, MONEY_COINS_COL_TOKEN_ID, MONEY_COINS_COL_VALUE,
    MONEY_COINS_COL_VALUE_BLIND, MONEY_COINS_TABLE, MONEY_KEYS_COL_IS_DEFAULT,
    MONEY_KEYS_COL_PUBLIC, MONEY_KEYS_COL_SECRET, MONEY_KEYS_TABLE, MONEY_TREE_COL_TREE,
    MONEY_TREE_TABLE,
};
use darkfi_sdk::{
    crypto::{
        constants::MERKLE_DEPTH, Keypair, MerkleNode, Nullifier, PublicKey, SecretKey, TokenId,
    },
    incrementalmerkletree,
    incrementalmerkletree::bridgetree::BridgeTree,
    pasta::pallas,
};
use darkfi_serial::{deserialize, serialize};
use prettytable::{format, row, Table};
use rand::rngs::OsRng;
use serde_json::json;

use super::Drk;

impl Drk {
    /// Initialize wallet with tables for the Money contract.
    /// This should be performed initially before doing other operations.
    pub async fn wallet_initialize(&self) -> Result<()> {
        let wallet_schema = include_str!("../../../src/contract/money/wallet.sql");

        // We perform a request to darkfid with the schema to initialize
        // the necessary tables in the wallet.
        let req = JsonRequest::new("wallet.exec_sql", json!([wallet_schema]));
        let rep = self.rpc_client.request(req).await?;

        if rep == true {
            println!("Successfully initialized wallet schema for Money Contract");
        } else {
            println!("Got unexpected reply from darkfid: {}", rep);
        }

        // Check if we have to initialize the Merkle tree.
        // We check if we find a row in the tree table, and if not, we create
        // a new tree and push it into the table.
        let mut tree_needs_init = false;
        let query = format!("SELECT * FROM {}", MONEY_TREE_TABLE);
        let params = json!([query, QueryType::Blob as u8, MONEY_TREE_COL_TREE]);
        let req = JsonRequest::new("wallet.query_row_single", params);

        // For now, on success, we don't care what's returned, but maybe in
        // the future we should actually check it?
        // TODO: The RPC needs a better variant for errors so detailed inspection
        //       can be done with error codes and all that.
        if let Err(_) = self.rpc_client.request(req).await {
            tree_needs_init = true;
        }

        if tree_needs_init {
            println!("Initializing Merkle tree");
            let tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100);
            self.put_tree(&tree).await?;
            println!("Successfully initialized Merkle tree");
        }

        Ok(())
    }

    /// Generate a new wallet keypair and put it in the according wallet table.
    pub async fn wallet_keygen(&self) -> Result<()> {
        println!("Generating a new keypair");
        // TODO: We might want to have hierarchical deterministic key derivation.
        let keypair = Keypair::random(&mut OsRng);
        let public = serialize(&keypair.public);
        let secret = serialize(&keypair.secret);
        let is_default = 0;

        let query = format!(
            "INSERT INTO {} ({}, {}, {}) VALUES (?1, ?2, ?3)",
            MONEY_KEYS_TABLE,
            MONEY_KEYS_COL_IS_DEFAULT,
            MONEY_KEYS_COL_PUBLIC,
            MONEY_KEYS_COL_SECRET,
        );

        let params = json!([
            query,
            QueryType::Integer as u8,
            is_default,
            QueryType::Blob as u8,
            public,
            QueryType::Blob as u8,
            secret,
        ]);

        let req = JsonRequest::new("wallet.exec_sql", params);
        let rep = self.rpc_client.request(req).await?;

        if rep == true {
            println!("Successfully added new keypair to wallet");
        } else {
            println!("Got unexpected reply from darkfid: {}", rep);
        }

        println!("New address: {}", keypair.public);
        Ok(())
    }

    /// Fetch all coins and their metadata from the wallet, optionally also spent ones.
    /// The boolean in the return tuple marks if the coin is marked as spent.
    pub async fn wallet_coins(&self, fetch_spent: bool) -> Result<Vec<(OwnCoin, bool)>> {
        eprintln!("Fetching OwnCoins from wallet");
        let query = if fetch_spent {
            format!("SELECT * FROM {}", MONEY_COINS_TABLE)
        } else {
            format!(
                "SELECT * FROM {} WHERE {} = {}",
                MONEY_COINS_TABLE, MONEY_COINS_COL_IS_SPENT, false,
            )
        };

        let params = json!([
            query,
            QueryType::Blob as u8,
            MONEY_COINS_COL_COIN,
            QueryType::Integer as u8,
            MONEY_COINS_COL_IS_SPENT,
            QueryType::Blob as u8,
            MONEY_COINS_COL_SERIAL,
            QueryType::Blob as u8,
            MONEY_COINS_COL_VALUE,
            QueryType::Blob as u8,
            MONEY_COINS_COL_TOKEN_ID,
            QueryType::Blob as u8,
            MONEY_COINS_COL_COIN_BLIND,
            QueryType::Blob as u8,
            MONEY_COINS_COL_VALUE_BLIND,
            QueryType::Blob as u8,
            MONEY_COINS_COL_TOKEN_BLIND,
            QueryType::Blob as u8,
            MONEY_COINS_COL_SECRET,
            QueryType::Blob as u8,
            MONEY_COINS_COL_NULLIFIER,
            QueryType::Blob as u8,
            MONEY_COINS_COL_LEAF_POSITION,
            QueryType::Blob as u8,
            MONEY_COINS_COL_MEMO,
        ]);

        let req = JsonRequest::new("wallet.query_row_multi", params);
        let rep = self.rpc_client.request(req).await?;

        // The returned thing should be an array of found rows.
        let Some(rows) = rep.as_array() else {
            return Err(anyhow!("Unexpected response from darkfid: {}", rep))
        };

        let mut owncoins = vec![];

        for row in rows {
            let Some(row) = row.as_array() else {
                return Err(anyhow!("Unexpected response from darkfid: {}", rep))
            };

            let coin_bytes: Vec<u8> = serde_json::from_value(row[0].clone())?;
            let coin: Coin = deserialize(&coin_bytes)?;

            let is_spent: u64 = serde_json::from_value(row[1].clone())?;
            let is_spent = if is_spent > 0 { true } else { false };

            let serial_bytes: Vec<u8> = serde_json::from_value(row[2].clone())?;
            let serial: pallas::Base = deserialize(&serial_bytes)?;

            let value_bytes: Vec<u8> = serde_json::from_value(row[3].clone())?;
            let value: u64 = deserialize(&value_bytes)?;

            let token_id_bytes: Vec<u8> = serde_json::from_value(row[4].clone())?;
            let token_id: TokenId = deserialize(&token_id_bytes)?;

            let coin_blind_bytes: Vec<u8> = serde_json::from_value(row[5].clone())?;
            let coin_blind: pallas::Base = deserialize(&coin_blind_bytes)?;

            let value_blind_bytes: Vec<u8> = serde_json::from_value(row[6].clone())?;
            let value_blind: pallas::Scalar = deserialize(&value_blind_bytes)?;

            let token_blind_bytes: Vec<u8> = serde_json::from_value(row[7].clone())?;
            let token_blind: pallas::Scalar = deserialize(&token_blind_bytes)?;

            let secret_bytes: Vec<u8> = serde_json::from_value(row[8].clone())?;
            let secret: SecretKey = deserialize(&secret_bytes)?;

            let nullifier_bytes: Vec<u8> = serde_json::from_value(row[9].clone())?;
            let nullifier: Nullifier = deserialize(&nullifier_bytes)?;

            let leaf_position_bytes: Vec<u8> = serde_json::from_value(row[10].clone())?;
            let leaf_position: incrementalmerkletree::Position = deserialize(&leaf_position_bytes)?;

            let memo: Vec<u8> = serde_json::from_value(row[11].clone())?;

            let note = Note { serial, value, token_id, coin_blind, value_blind, token_blind, memo };
            let owncoin = OwnCoin { coin, note, secret, nullifier, leaf_position };

            owncoins.push((owncoin, is_spent))
        }

        Ok(owncoins)
    }

    /// Fetch known balances from the wallet and try to print them as a table.
    pub async fn wallet_balance(&self) -> Result<()> {
        // This represents "false"
        let is_spent = 0;

        let query = format!(
            "SELECT {}, {} FROM {} WHERE {} = {}",
            MONEY_COINS_COL_VALUE,
            MONEY_COINS_COL_TOKEN_ID,
            MONEY_COINS_TABLE,
            MONEY_COINS_COL_IS_SPENT,
            is_spent,
        );

        let params = json!([
            query,
            QueryType::Blob as u8,
            MONEY_COINS_COL_VALUE,
            QueryType::Blob as u8,
            MONEY_COINS_COL_TOKEN_ID,
        ]);

        let req = JsonRequest::new("wallet.query_row_multi", params);
        let rep = self.rpc_client.request(req).await?;

        // The returned thing should be an array of found rows.
        let Some(rows) = rep.as_array() else {
            return Err(anyhow!("Unexpected response from darkfid: {}", rep))
        };

        // Fill this map with balances, and in the end we'll print it as a table.
        let mut balmap: HashMap<String, u64> = HashMap::new();

        // Let's scan through the rows and see if we got anything.
        for row in rows {
            let Some(row) = row.as_array() else {
                return Err(anyhow!("Unexpected response from darkfid: {}", rep))
            };

            if row.len() != 2 {
                eprintln!("Error: Got invalid array, row should contain two elements.");
                eprintln!("Actual contents:\n:{:#?}", row);
                return Err(anyhow!("Unexpected response from darkfid: {}", rep))
            }

            let value_bytes: Vec<u8> = serde_json::from_value(row[0].clone())?;
            let mut value: u64 = deserialize(&value_bytes)?;

            let token_bytes: Vec<u8> = serde_json::from_value(row[1].clone())?;
            let token_id: TokenId = deserialize(&token_bytes)?;
            let token_id = format!("{}", token_id);

            if let Some(prev) = balmap.get(&token_id) {
                value += prev;
            }

            balmap.insert(token_id, value);
        }

        // Create a prettytable with the new data.
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
        table.set_titles(row!["Token ID", "Balance"]);

        for (token_id, balance) in balmap.iter() {
            // FIXME: Don't hardcode to 8 decimals
            table.add_row(row![token_id, encode_base10(*balance, 8)]);
        }

        if table.is_empty() {
            println!("No unspent balances found");
        } else {
            println!("{}", table);
        }

        Ok(())
    }

    /// Fetch pubkeys from the wallet and print the requested index.
    pub async fn wallet_address(&self, _idx: u64) -> Result<PublicKey> {
        let query = format!("SELECT {} FROM {};", MONEY_KEYS_COL_PUBLIC, MONEY_KEYS_TABLE);
        let params = json!([query, QueryType::Blob as u8, MONEY_KEYS_COL_PUBLIC]);
        let req = JsonRequest::new("wallet.query_row_single", params);
        let rep = self.rpc_client.request(req).await?;

        let Some(arr) = rep.as_array() else {
            return Err(anyhow!("Unexpected response from darkfid: {}", rep));
        };

        if arr.len() != 1 {
            return Err(anyhow!("Unexpected response from darkfid: {}", rep))
        }

        let key_bytes: Vec<u8> = serde_json::from_value(arr[0].clone())?;
        let public_key: PublicKey = deserialize(&key_bytes)?;

        Ok(public_key)
    }

    /// Fetch secret keys from the wallet and return them if found.
    pub async fn wallet_secrets(&self) -> Result<Vec<SecretKey>> {
        let query = format!("SELECT {} FROM {};", MONEY_KEYS_COL_SECRET, MONEY_KEYS_TABLE);
        let params = json!([query, QueryType::Blob as u8, MONEY_KEYS_COL_SECRET]);
        let req = JsonRequest::new("wallet.query_row_multi", params);
        let rep = self.rpc_client.request(req).await?;

        // The returned thing should be an array of found rows.
        let Some(rows) = rep.as_array() else {
            return Err(anyhow!("Unexpected response from darkfid: {}", rep))
        };

        let mut secrets = vec![];

        // Let's scan through the rows and see if we got anything.
        for row in rows {
            let secret_bytes: Vec<u8> = serde_json::from_value(row[0].clone())?;
            let secret: SecretKey = deserialize(&secret_bytes)?;
            secrets.push(secret);
        }

        Ok(secrets)
    }

    /// Get the Merkle tree from the wallet
    pub async fn wallet_tree(&self) -> Result<BridgeTree<MerkleNode, MERKLE_DEPTH>> {
        let query = format!("SELECT * FROM {}", MONEY_TREE_TABLE);
        let params = json!([query, QueryType::Blob as u8, MONEY_TREE_COL_TREE]);
        let req = JsonRequest::new("wallet.query_row_single", params);
        let rep = self.rpc_client.request(req).await?;

        let tree_bytes: Vec<u8> = serde_json::from_value(rep[0].clone())?;
        let tree = deserialize(&tree_bytes)?;
        Ok(tree)
    }

    /// Mark a coin in the wallet as spent
    pub async fn mark_spent_coin(&self, coin: &Coin) -> Result<()> {
        let query = format!(
            "UPDATE {} SET {} = ?1 WHERE {} = ?2;",
            MONEY_COINS_TABLE, MONEY_COINS_COL_IS_SPENT, MONEY_COINS_COL_COIN
        );

        let params = json!([
            query,
            QueryType::Integer as u8,
            1,
            QueryType::Blob as u8,
            serialize(&coin.inner())
        ]);

        let req = JsonRequest::new("wallet.exec_sql", params);
        let _ = self.rpc_client.request(req).await?;

        Ok(())
    }

    /// Mark a given coin in the wallet as unspent
    pub async fn unspend_coin(&self, coin: &Coin) -> Result<()> {
        let query = format!(
            "UPDATE {} SET {} = ?1 WHERE {} = ?2;",
            MONEY_COINS_TABLE, MONEY_COINS_COL_IS_SPENT, MONEY_COINS_COL_COIN
        );

        let params = json!([
            query,
            QueryType::Integer as u8,
            0,
            QueryType::Blob as u8,
            serialize(&coin.inner())
        ]);

        let req = JsonRequest::new("wallet.exec_sql", params);
        let _ = self.rpc_client.request(req).await?;

        Ok(())
    }

    /// Replace the Merkle tree in the wallet
    pub async fn put_tree(&self, tree: &BridgeTree<MerkleNode, MERKLE_DEPTH>) -> Result<()> {
        let query = format!(
            "DELETE FROM {}; INSERT INTO {} ({}) VALUES (?1);",
            MONEY_TREE_TABLE, MONEY_TREE_TABLE, MONEY_TREE_COL_TREE
        );

        let params = json!([query, QueryType::Blob as u8, serialize(tree)]);
        let req = JsonRequest::new("wallet.exec_sql", params);
        let _ = self.rpc_client.request(req).await?;

        Ok(())
    }
}
