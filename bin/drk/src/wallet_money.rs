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
use std::collections::HashMap;

use anyhow::{anyhow, Result};
use darkfi::{rpc::jsonrpc::JsonRequest, tx::Transaction, wallet::walletdb::QueryType};
use darkfi_money_contract::{
    client::{
        Coin, EncryptedNote, Note, OwnCoin, MONEY_ALIASES_COL_ALIAS, MONEY_ALIASES_COL_TOKEN_ID,
        MONEY_ALIASES_TABLE, MONEY_COINS_COL_COIN, MONEY_COINS_COL_COIN_BLIND,
        MONEY_COINS_COL_IS_SPENT, MONEY_COINS_COL_LEAF_POSITION, MONEY_COINS_COL_MEMO,
        MONEY_COINS_COL_NULLIFIER, MONEY_COINS_COL_SECRET, MONEY_COINS_COL_SERIAL,
        MONEY_COINS_COL_SPEND_HOOK, MONEY_COINS_COL_TOKEN_BLIND, MONEY_COINS_COL_TOKEN_ID,
        MONEY_COINS_COL_USER_DATA, MONEY_COINS_COL_VALUE, MONEY_COINS_COL_VALUE_BLIND,
        MONEY_COINS_TABLE, MONEY_INFO_COL_LAST_SCANNED_SLOT, MONEY_INFO_TABLE,
        MONEY_KEYS_COL_IS_DEFAULT, MONEY_KEYS_COL_KEY_ID, MONEY_KEYS_COL_PUBLIC,
        MONEY_KEYS_COL_SECRET, MONEY_KEYS_TABLE, MONEY_TREE_COL_TREE, MONEY_TREE_TABLE,
    },
    model::{MoneyTransferParams, Output},
    MoneyFunction,
};
use darkfi_sdk::{
    crypto::{
        poseidon_hash, Keypair, MerkleNode, MerkleTree, Nullifier, PublicKey, SecretKey, TokenId,
        MONEY_CONTRACT_ID,
    },
    incrementalmerkletree,
    incrementalmerkletree::Tree,
    pasta::pallas,
};
use darkfi_serial::{deserialize, serialize};
use rand::rngs::OsRng;
use serde_json::json;

use super::Drk;
use crate::cli_util::kaching;

impl Drk {
    /// Initialize wallet with tables for the Money contract
    pub async fn initialize_money(&self) -> Result<()> {
        let wallet_schema = include_str!("../../../src/contract/money/wallet.sql");

        // We perform a request to darkfid with the schema to initialize
        // the necessary tables in the wallet.
        let req = JsonRequest::new("wallet.exec_sql", json!([wallet_schema]));
        let rep = self.rpc_client.request(req).await?;

        if rep == true {
            eprintln!("Successfully initialized wallet schema for the Money contract");
        } else {
            eprintln!("[initialize_money] Got unexpected reply from darkfid: {}", rep);
        }

        // Check if we have to initialize the Merkle tree.
        // We check if we find a row in the tree table, and if not, we create a
        // new tree and push it into the table.
        let mut tree_needs_init = false;
        let query = format!("SELECT {} FROM {}", MONEY_TREE_COL_TREE, MONEY_TREE_TABLE);
        let params = json!([query, QueryType::Blob as u8, MONEY_TREE_COL_TREE]);
        let req = JsonRequest::new("wallet.query_row_single", params);

        // For now, on success, we don't care what's returned, but in the future
        // we should actually check it.
        // TODO: The RPC needs a better variant for errors so detailed inspection
        //       can be done with error codes and all that.
        if (self.rpc_client.request(req).await).is_err() {
            tree_needs_init = true;
        }

        if tree_needs_init {
            eprintln!("Initializing Money Merkle tree");
            let tree = MerkleTree::new(100);
            self.put_money_tree(&tree).await?;
            eprintln!("Successfully initialized Merkle tree for the Money contract");
        }

        // We maintain the last scanned slot as part of the Money contract,
        // but at this moment it is also somewhat applicable to DAO scans.
        if (self.last_scanned_slot().await).is_err() {
            let query = format!(
                "INSERT INTO {} ({}) VALUES (?1);",
                MONEY_INFO_TABLE, MONEY_INFO_COL_LAST_SCANNED_SLOT
            );

            let params = json!([query, QueryType::Integer as u8, 0]);
            let req = JsonRequest::new("wallet.exec_sql", params);
            let _ = self.rpc_client.request(req).await?;
        }

        Ok(())
    }

    /// Generate a new keypair and place it into the wallet.
    pub async fn money_keygen(&self) -> Result<()> {
        eprintln!("Generating a new keypair");
        // TODO: We might want to have hierarchical deterministic key derivation.
        let keypair = Keypair::random(&mut OsRng);
        let is_default = 0;

        let query = format!(
            "INSERT INTO {} ({}, {}, {}) VALUES (?1, ?2, ?3);",
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
            serialize(&keypair.public),
            QueryType::Blob as u8,
            serialize(&keypair.secret),
        ]);

        let req = JsonRequest::new("wallet.exec_sql", params);
        let rep = self.rpc_client.request(req).await?;

        if rep == true {
            eprintln!("Successfully added new keypair to wallet");
        } else {
            eprintln!("[money_keygen] Got unexpected reply from darkfid: {}", rep);
        }

        eprintln!("New address:");
        println!("{}", keypair.public);

        Ok(())
    }

    /// Fetch all secret keys from the wallet
    pub async fn get_money_secrets(&self) -> Result<Vec<SecretKey>> {
        let query = format!("SELECT {} FROM {};", MONEY_KEYS_COL_SECRET, MONEY_KEYS_TABLE);
        let params = json!([query, QueryType::Blob as u8, MONEY_KEYS_COL_SECRET]);
        let req = JsonRequest::new("wallet.query_row_multi", params);
        let rep = self.rpc_client.request(req).await?;

        // The returned thing should be an array of found rows.
        let Some(rows) = rep.as_array() else {
            return Err(anyhow!("[get_money_secrets] Unexpected response from darkfid: {}", rep));
        };

        let mut secrets = Vec::with_capacity(rows.len());

        // Let's scan through the rows and see if we got anything.
        for row in rows {
            let secret_bytes: Vec<u8> = serde_json::from_value(row[0].clone())?;
            let secret = deserialize(&secret_bytes)?;
            secrets.push(secret);
        }

        Ok(secrets)
    }

    /// Import given secret keys into the wallet.
    /// The query uses INSERT, so if the key already exists, it will be skipped.
    /// Returns the respective PublicKey objects for the imported keys.
    pub async fn import_money_secrets(&self, secrets: Vec<SecretKey>) -> Result<Vec<PublicKey>> {
        let mut ret = Vec::with_capacity(secrets.len());

        for secret in secrets {
            ret.push(PublicKey::from_secret(secret));
            let is_default = 0;
            let public = serialize(&PublicKey::from_secret(secret));
            let secret = serialize(&secret);

            let query = format!(
                "INSERT INTO {} ({}, {}, {}) VALUES (?1, ?2, ?3);",
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
            let _ = self.rpc_client.request(req).await?;
        }

        Ok(ret)
    }

    /// Fetch pubkeys from the wallet and return the requested index.
    pub async fn wallet_address(&self, idx: u64) -> Result<PublicKey> {
        let query = format!(
            "SELECT {} FROM {} WHERE {} = {};",
            MONEY_KEYS_COL_PUBLIC, MONEY_KEYS_TABLE, MONEY_KEYS_COL_KEY_ID, idx
        );

        let params = json!([query, QueryType::Blob as u8, MONEY_KEYS_COL_PUBLIC]);
        let req = JsonRequest::new("wallet.query_row_single", params);
        let rep = self.rpc_client.request(req).await?;

        let Some(arr) = rep.as_array() else {
            return Err(anyhow!("[wallet_address] Unexpected response from darkfid: {}", rep))
        };

        if arr.len() != 1 {
            return Err(anyhow!("Did not find pubkey with index {}", idx))
        }

        let key_bytes: Vec<u8> = serde_json::from_value(arr[0].clone())?;
        let public_key: PublicKey = deserialize(&key_bytes)?;

        Ok(public_key)
    }

    /// Fetch all coins and their metadata related to the Money contract from the wallet.
    /// Optionally also fetch spent ones.
    /// The boolean in the returned tuple notes if the coin was marked as spent.
    pub async fn get_coins(&self, fetch_spent: bool) -> Result<Vec<(OwnCoin, bool)>> {
        eprintln!("Fetching OwnCoins from the wallet");

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
            MONEY_COINS_COL_SPEND_HOOK,
            QueryType::Blob as u8,
            MONEY_COINS_COL_USER_DATA,
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
            return Err(anyhow!("[get_coins] Unexpected response from darkfid: {}", rep))
        };

        let mut owncoins = Vec::with_capacity(rows.len());

        for row in rows {
            let Some(row) = row.as_array() else {
                return Err(anyhow!("[get_coins] Unexpected response from darkfid: {}", rep))
            };

            let coin_bytes: Vec<u8> = serde_json::from_value(row[0].clone())?;
            let coin: Coin = deserialize(&coin_bytes)?;

            let is_spent: u64 = serde_json::from_value(row[1].clone())?;
            let is_spent = is_spent > 0;

            let serial_bytes: Vec<u8> = serde_json::from_value(row[2].clone())?;
            let serial: pallas::Base = deserialize(&serial_bytes)?;

            let value_bytes: Vec<u8> = serde_json::from_value(row[3].clone())?;
            let value: u64 = deserialize(&value_bytes)?;

            let token_id_bytes: Vec<u8> = serde_json::from_value(row[4].clone())?;
            let token_id: TokenId = deserialize(&token_id_bytes)?;

            let spend_hook_bytes: Vec<u8> = serde_json::from_value(row[5].clone())?;
            let spend_hook: pallas::Base = deserialize(&spend_hook_bytes)?;

            let user_data_bytes: Vec<u8> = serde_json::from_value(row[6].clone())?;
            let user_data: pallas::Base = deserialize(&user_data_bytes)?;

            let coin_blind_bytes: Vec<u8> = serde_json::from_value(row[7].clone())?;
            let coin_blind: pallas::Base = deserialize(&coin_blind_bytes)?;

            let value_blind_bytes: Vec<u8> = serde_json::from_value(row[8].clone())?;
            let value_blind: pallas::Scalar = deserialize(&value_blind_bytes)?;

            let token_blind_bytes: Vec<u8> = serde_json::from_value(row[9].clone())?;
            let token_blind: pallas::Scalar = deserialize(&token_blind_bytes)?;

            let secret_bytes: Vec<u8> = serde_json::from_value(row[10].clone())?;
            let secret: SecretKey = deserialize(&secret_bytes)?;

            let nullifier_bytes: Vec<u8> = serde_json::from_value(row[11].clone())?;
            let nullifier: Nullifier = deserialize(&nullifier_bytes)?;

            let leaf_position_bytes: Vec<u8> = serde_json::from_value(row[12].clone())?;
            let leaf_position: incrementalmerkletree::Position = deserialize(&leaf_position_bytes)?;

            let memo: Vec<u8> = serde_json::from_value(row[13].clone())?;

            let note = Note {
                serial,
                value,
                token_id,
                spend_hook,
                user_data,
                coin_blind,
                value_blind,
                token_blind,
                memo,
            };
            let owncoin = OwnCoin { coin, note, secret, nullifier, leaf_position };

            owncoins.push((owncoin, is_spent))
        }

        Ok(owncoins)
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

    /// Marks all coins in the wallet as spent, if their nullifier is in the given set
    pub async fn mark_spent_coins(&self, nullifiers: &[Nullifier]) -> Result<()> {
        if nullifiers.is_empty() {
            return Ok(())
        }

        for (coin, _) in self.get_coins(false).await? {
            if nullifiers.contains(&coin.nullifier) {
                self.mark_spent_coin(&coin.coin).await?;
            }
        }

        Ok(())
    }

    /// Mark a given coin in the wallet as unspent
    pub async fn unspend_coin(&self, coin: &Coin) -> Result<()> {
        let query = format!(
            "UPDATE {} SET {} = ?1 WHERE {} = ?2;",
            MONEY_COINS_TABLE, MONEY_COINS_COL_IS_SPENT, MONEY_COINS_COL_COIN,
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

    /// Replace the Money Merkle tree in the wallet.
    pub async fn put_money_tree(&self, tree: &MerkleTree) -> Result<()> {
        let query = format!(
            "DELETE FROM {}; INSERT INTO {} ({}) VALUES (?1);",
            MONEY_TREE_TABLE, MONEY_TREE_TABLE, MONEY_TREE_COL_TREE,
        );

        let params = json!([query, QueryType::Blob as u8, serialize(tree)]);

        let req = JsonRequest::new("wallet.exec_sql", params);
        let _ = self.rpc_client.request(req).await?;

        Ok(())
    }

    /// Fetch the Money Merkle tree from the wallet
    pub async fn get_money_tree(&self) -> Result<MerkleTree> {
        let query = format!("SELECT * FROM {}", MONEY_TREE_TABLE);
        let params = json!([query, QueryType::Blob as u8, MONEY_TREE_COL_TREE]);
        let req = JsonRequest::new("wallet.query_row_single", params);
        let rep = self.rpc_client.request(req).await?;

        let tree_bytes: Vec<u8> = serde_json::from_value(rep[0].clone())?;
        let tree = deserialize(&tree_bytes)?;
        Ok(tree)
    }

    /// Reset the Money Merkle tree in the wallet
    pub async fn reset_money_tree(&self) -> Result<()> {
        eprintln!("Resetting Money Merkle tree");
        let tree = MerkleTree::new(100);
        self.put_money_tree(&tree).await?;
        eprintln!("Successfully reset Money Merkle tree");

        Ok(())
    }

    /// Reset the Money coins in the wallet
    pub async fn reset_money_coins(&self) -> Result<()> {
        eprintln!("Resetting coins");
        let query = format!("DELETE FROM {};", MONEY_COINS_TABLE);
        let params = json!([query]);
        let req = JsonRequest::new("wallet.exec_sql", params);
        let _ = self.rpc_client.request(req).await?;
        eprintln!("Successfully reset coins");

        Ok(())
    }

    /// Fetch known unspent balances from the wallet and return them as a hashmap.
    pub async fn money_balance(&self) -> Result<HashMap<String, u64>> {
        let mut coins = self.get_coins(false).await?;
        coins.retain(|x| x.0.note.spend_hook == pallas::Base::zero());

        // Fill this map with balances
        let mut balmap: HashMap<String, u64> = HashMap::new();

        for coin in coins {
            let mut value = coin.0.note.value;

            if let Some(prev) = balmap.get(&coin.0.note.token_id.to_string()) {
                value += prev;
            }

            balmap.insert(coin.0.note.token_id.to_string(), value);
        }

        Ok(balmap)
    }

    /// Append data related to Money contract transactions into the wallet database.
    pub async fn apply_tx_money_data(&self, tx: &Transaction, _confirm: bool) -> Result<()> {
        let cid = *MONEY_CONTRACT_ID;

        let mut nullifiers: Vec<Nullifier> = vec![];
        let mut outputs: Vec<Output> = vec![];

        for (i, call) in tx.calls.iter().enumerate() {
            if call.contract_id == cid && call.data[0] == MoneyFunction::Transfer as u8 {
                eprintln!("Found Money::Transfer in call {}", i);
                let params: MoneyTransferParams = deserialize(&call.data[1..])?;

                for input in params.inputs {
                    nullifiers.push(input.nullifier);
                }

                for output in params.outputs {
                    outputs.push(output);
                }

                continue
            }

            if call.contract_id == cid && call.data[0] == MoneyFunction::OtcSwap as u8 {
                eprintln!("Found Money::OtcSwap in call {}", i);
                let params: MoneyTransferParams = deserialize(&call.data[1..])?;

                for input in params.inputs {
                    nullifiers.push(input.nullifier);
                }

                for output in params.outputs {
                    outputs.push(output);
                }

                continue
            }
        }

        let secrets = self.get_money_secrets().await?;
        let dao_secrets = self.get_dao_secrets().await?;
        let mut tree = self.get_money_tree().await?;

        let mut owncoins = vec![];

        for output in outputs {
            let coin = output.coin;

            // Append the new coin to the Merkle tree. Every coin has to be added.
            tree.append(&MerkleNode::from(coin));

            // Attempt to decrypt the note
            let enc_note =
                EncryptedNote { ciphertext: output.ciphertext, ephem_public: output.ephem_public };

            for secret in secrets.iter().chain(dao_secrets.iter()) {
                if let Ok(note) = enc_note.decrypt(secret) {
                    eprintln!("Successfully decrypted a Money Note");
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

        self.put_money_tree(&tree).await?;
        if !nullifiers.is_empty() {
            self.mark_spent_coins(&nullifiers).await?;
        }

        // This is the SQL query we'll be executing to insert new coins
        // into the wallet
        let query = format!(
            "INSERT INTO {} ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14);",
            MONEY_COINS_TABLE,
            MONEY_COINS_COL_COIN,
            MONEY_COINS_COL_IS_SPENT,
            MONEY_COINS_COL_SERIAL,
            MONEY_COINS_COL_VALUE,
            MONEY_COINS_COL_TOKEN_ID,
            MONEY_COINS_COL_SPEND_HOOK,
            MONEY_COINS_COL_USER_DATA,
            MONEY_COINS_COL_COIN_BLIND,
            MONEY_COINS_COL_VALUE_BLIND,
            MONEY_COINS_COL_TOKEN_BLIND,
            MONEY_COINS_COL_SECRET,
            MONEY_COINS_COL_NULLIFIER,
            MONEY_COINS_COL_LEAF_POSITION,
            MONEY_COINS_COL_MEMO,
        );

        eprintln!("Found {} OwnCoin(s) in transaction", owncoins.len());
        for owncoin in &owncoins {
            eprintln!("OwnCoin: {:?}", owncoin.coin);
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
                serialize(&owncoin.note.spend_hook),
                QueryType::Blob as u8,
                serialize(&owncoin.note.user_data),
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

            let req = JsonRequest::new("wallet.exec_sql", params);
            let _ = self.rpc_client.request(req).await?;
        }

        if !owncoins.is_empty() {
            if let Err(_) = kaching().await {
                return Ok(())
            }
        }

        Ok(())
    }

    /// Get the last scanned slot from the wallet
    pub async fn last_scanned_slot(&self) -> Result<u64> {
        let query =
            format!("SELECT {} FROM {};", MONEY_INFO_COL_LAST_SCANNED_SLOT, MONEY_INFO_TABLE);

        let params = json!([query, QueryType::Integer as u8, MONEY_INFO_COL_LAST_SCANNED_SLOT]);
        let req = JsonRequest::new("wallet.query_row_single", params);
        let rep = self.rpc_client.request(req).await?;

        Ok(serde_json::from_value(rep[0].clone())?)
    }

    /// Create an alias record for provided Token ID
    pub async fn add_alias(&self, alias: String, token_id: TokenId) -> Result<()> {
        eprintln!("Generating alias {} for Token: {}", alias, token_id);
        let query = format!(
            "INSERT OR REPLACE INTO {} ({}, {}) VALUES (?1, ?2);",
            MONEY_ALIASES_TABLE, MONEY_ALIASES_COL_ALIAS, MONEY_ALIASES_COL_TOKEN_ID,
        );

        let params = json!([
            query,
            QueryType::Blob as u8,
            serialize(&alias),
            QueryType::Blob as u8,
            serialize(&token_id),
        ]);

        let req = JsonRequest::new("wallet.exec_sql", params);
        let rep = self.rpc_client.request(req).await?;

        if rep == true {
            eprintln!("Successfully added new alias to wallet");
        } else {
            eprintln!("[add_alias] Got unexpected reply from darkfid: {}", rep);
        }

        Ok(())
    }

    /// Fetch all aliases from the wallet.
    /// Optionally filter using alias name and/or token id.
    pub async fn get_aliases(
        &self,
        alias_filter: Option<String>,
        token_id_filter: Option<TokenId>,
    ) -> Result<HashMap<String, TokenId>> {
        eprintln!("Fetching Aliases from the wallet");

        let query = format!("SELECT * FROM {}", MONEY_ALIASES_TABLE);
        let params = json!([
            query,
            QueryType::Blob as u8,
            MONEY_ALIASES_COL_ALIAS,
            QueryType::Blob as u8,
            MONEY_ALIASES_COL_TOKEN_ID,
        ]);

        let req = JsonRequest::new("wallet.query_row_multi", params);
        let rep = self.rpc_client.request(req).await?;

        // The returned thing should be an array of found rows.
        let Some(rows) = rep.as_array() else {
            return Err(anyhow!("[get_aliases] Unexpected response from darkfid: {}", rep))
        };

        // Fill this map with aliases
        let mut map: HashMap<String, TokenId> = HashMap::new();
        for row in rows {
            let Some(row) = row.as_array() else {
                return Err(anyhow!("[get_aliases] Unexpected response from darkfid: {}", rep))
            };

            let alias_bytes: Vec<u8> = serde_json::from_value(row[0].clone())?;
            let alias: String = deserialize(&alias_bytes)?;
            if alias_filter.is_some() && alias_filter.as_ref().unwrap() != &alias {
                continue
            }

            let token_id_bytes: Vec<u8> = serde_json::from_value(row[1].clone())?;
            let token_id: TokenId = deserialize(&token_id_bytes)?;
            if token_id_filter.is_some() && token_id_filter.as_ref().unwrap() != &token_id {
                continue
            }

            map.insert(alias, token_id);
        }

        Ok(map)
    }

    /// Fetch all aliases from the wallet, mapped by token id.
    pub async fn get_aliases_mapped_by_token(&self) -> Result<HashMap<String, String>> {
        let aliases = self.get_aliases(None, None).await?;
        let mut map: HashMap<String, String> = HashMap::new();
        for (alias, token_id) in aliases {
            let aliases_string = if let Some(prev) = map.get(&token_id.to_string()) {
                format!("{}, {}", prev, alias)
            } else {
                alias
            };

            map.insert(token_id.to_string(), aliases_string);
        }

        Ok(map)
    }

    /// Retrieve token by provided string.
    /// Input string represents either an alias or a token id.
    pub async fn get_token(&self, input: String) -> Result<TokenId> {
        // Check if input is an alias(max 5 characters)
        if input.chars().count() <= 5 {
            let aliases = self.get_aliases(Some(input.clone()), None).await?;
            if let Some(token_id) = aliases.get(&input) {
                return Ok(token_id.clone())
            }
        }
        // Else parse input
        Ok(TokenId::try_from(input.as_str())?)
    }

    /// Create an alias record for provided Token ID
    pub async fn remove_alias(&self, alias: String) -> Result<()> {
        eprintln!("Removing alias: {}", alias);
        let query =
            format!("DELETE FROM {} WHERE {} = ?1;", MONEY_ALIASES_TABLE, MONEY_ALIASES_COL_ALIAS,);

        let params = json!([query, QueryType::Blob as u8, serialize(&alias),]);

        let req = JsonRequest::new("wallet.exec_sql", params);
        let rep = self.rpc_client.request(req).await?;

        if rep == true {
            eprintln!("Successfully removed alias from wallet");
        } else {
            eprintln!("[remove_alias] Got unexpected reply from darkfid: {}", rep);
        }

        Ok(())
    }
}
