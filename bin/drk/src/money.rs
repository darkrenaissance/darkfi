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

use std::{collections::HashMap, str::FromStr};

use lazy_static::lazy_static;
use rand::rngs::OsRng;
use rusqlite::types::Value;

use darkfi::{tx::Transaction, zk::halo2::Field, Error, Result};
use darkfi_money_contract::{
    client::{MoneyNote, OwnCoin},
    model::{Coin, MoneyTokenFreezeParamsV1, MoneyTokenMintParamsV1, MoneyTransferParamsV1},
    MoneyFunction,
};
use darkfi_sdk::{
    bridgetree,
    crypto::{
        note::AeadEncryptedNote, poseidon_hash, FuncId, Keypair, MerkleNode, MerkleTree, Nullifier,
        PublicKey, SecretKey, TokenId, MONEY_CONTRACT_ID,
    },
    pasta::pallas,
};
use darkfi_serial::{deserialize, serialize};

use crate::{
    convert_named_params,
    error::{WalletDbError, WalletDbResult},
    kaching, Drk,
};

// Wallet SQL table constant names. These have to represent the `wallet.sql`
// SQL schema. Table names are prefixed with the contract ID to avoid collisions.
lazy_static! {
    pub static ref MONEY_INFO_TABLE: String =
        format!("{}_money_info", MONEY_CONTRACT_ID.to_string());
    pub static ref MONEY_TREE_TABLE: String =
        format!("{}_money_tree", MONEY_CONTRACT_ID.to_string());
    pub static ref MONEY_KEYS_TABLE: String =
        format!("{}_money_keys", MONEY_CONTRACT_ID.to_string());
    pub static ref MONEY_COINS_TABLE: String =
        format!("{}_money_coins", MONEY_CONTRACT_ID.to_string());
    pub static ref MONEY_TOKENS_TABLE: String =
        format!("{}_money_tokens", MONEY_CONTRACT_ID.to_string());
    pub static ref MONEY_ALIASES_TABLE: String =
        format!("{}_money_aliases", MONEY_CONTRACT_ID.to_string());
}

// MONEY_INFO_TABLE
pub const MONEY_INFO_COL_LAST_SCANNED_SLOT: &str = "last_scanned_slot";

// MONEY_TREE_TABLE
pub const MONEY_TREE_COL_TREE: &str = "tree";

// MONEY_KEYS_TABLE
pub const MONEY_KEYS_COL_KEY_ID: &str = "key_id";
pub const MONEY_KEYS_COL_IS_DEFAULT: &str = "is_default";
pub const MONEY_KEYS_COL_PUBLIC: &str = "public";
pub const MONEY_KEYS_COL_SECRET: &str = "secret";

// MONEY_COINS_TABLE
pub const MONEY_COINS_COL_COIN: &str = "coin";
pub const MONEY_COINS_COL_IS_SPENT: &str = "is_spent";
pub const MONEY_COINS_COL_SERIAL: &str = "serial";
pub const MONEY_COINS_COL_VALUE: &str = "value";
pub const MONEY_COINS_COL_TOKEN_ID: &str = "token_id";
pub const MONEY_COINS_COL_SPEND_HOOK: &str = "spend_hook";
pub const MONEY_COINS_COL_USER_DATA: &str = "user_data";
pub const MONEY_COINS_COL_VALUE_BLIND: &str = "value_blind";
pub const MONEY_COINS_COL_TOKEN_BLIND: &str = "token_blind";
pub const MONEY_COINS_COL_SECRET: &str = "secret";
pub const MONEY_COINS_COL_NULLIFIER: &str = "nullifier";
pub const MONEY_COINS_COL_LEAF_POSITION: &str = "leaf_position";
pub const MONEY_COINS_COL_MEMO: &str = "memo";

// MONEY_TOKENS_TABLE
pub const MONEY_TOKENS_COL_MINT_AUTHORITY: &str = "mint_authority";
pub const MONEY_TOKENS_COL_TOKEN_ID: &str = "token_id";
pub const MONEY_TOKENS_COL_IS_FROZEN: &str = "is_frozen";

// MONEY_ALIASES_TABLE
pub const MONEY_ALIASES_COL_ALIAS: &str = "alias";
pub const MONEY_ALIASES_COL_TOKEN_ID: &str = "token_id";

pub const BALANCE_BASE10_DECIMALS: usize = 8;

impl Drk {
    /// Initialize wallet with tables for the Money contract.
    pub async fn initialize_money(&self) -> WalletDbResult<()> {
        // Initialize Money wallet schema
        let wallet_schema = include_str!("../money.sql");
        self.wallet.exec_batch_sql(wallet_schema).await?;

        // Check if we have to initialize the Merkle tree.
        // We check if we find a row in the tree table, and if not, we create a
        // new tree and push it into the table.
        // For now, on success, we don't care what's returned, but in the future
        // we should actually check it.
        if self.get_money_tree().await.is_err() {
            eprintln!("Initializing Money Merkle tree");
            let mut tree = MerkleTree::new(100);
            tree.append(MerkleNode::from(pallas::Base::ZERO));
            let _ = tree.mark().unwrap();
            self.put_money_tree(&tree).await?;
            eprintln!("Successfully initialized Merkle tree for the Money contract");
        }

        // We maintain the last scanned slot as part of the Money contract,
        // but at this moment it is also somewhat applicable to DAO scans.
        if self.last_scanned_slot().await.is_err() {
            let query = format!(
                "INSERT INTO {} ({}) VALUES (?1);",
                *MONEY_INFO_TABLE, MONEY_INFO_COL_LAST_SCANNED_SLOT
            );
            self.wallet.exec_sql(&query, rusqlite::params![0]).await?;
        }

        Ok(())
    }

    /// Generate a new keypair and place it into the wallet.
    pub async fn money_keygen(&self) -> WalletDbResult<()> {
        eprintln!("Generating a new keypair");

        // TODO: We might want to have hierarchical deterministic key derivation.
        let keypair = Keypair::random(&mut OsRng);
        let is_default = 0;

        let query = format!(
            "INSERT INTO {} ({}, {}, {}) VALUES (?1, ?2, ?3);",
            *MONEY_KEYS_TABLE,
            MONEY_KEYS_COL_IS_DEFAULT,
            MONEY_KEYS_COL_PUBLIC,
            MONEY_KEYS_COL_SECRET
        );
        self.wallet
            .exec_sql(
                &query,
                rusqlite::params![
                    is_default,
                    serialize(&keypair.public),
                    serialize(&keypair.secret)
                ],
            )
            .await?;

        eprintln!("New address:");
        eprintln!("{}", keypair.public);

        Ok(())
    }

    /// Fetch default secret key from the wallet.
    pub async fn default_secret(&self) -> Result<SecretKey> {
        let row = match self
            .wallet
            .query_single(
                &MONEY_KEYS_TABLE,
                &[MONEY_KEYS_COL_SECRET],
                convert_named_params! {(MONEY_KEYS_COL_IS_DEFAULT, 1)},
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[default_secret] Default secret key retrieval failed: {e:?}"
                )))
            }
        };

        let Value::Blob(ref key_bytes) = row[0] else {
            return Err(Error::ParseFailed("[default_secret] Key bytes parsing failed"))
        };
        let secret_key: SecretKey = deserialize(key_bytes)?;

        Ok(secret_key)
    }

    /// Fetch default pubkey from the wallet.
    pub async fn default_address(&self) -> Result<PublicKey> {
        let row = match self
            .wallet
            .query_single(
                &MONEY_KEYS_TABLE,
                &[MONEY_KEYS_COL_PUBLIC],
                convert_named_params! {(MONEY_KEYS_COL_IS_DEFAULT, 1)},
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[default_address] Default address retrieval failed: {e:?}"
                )))
            }
        };

        let Value::Blob(ref key_bytes) = row[0] else {
            return Err(Error::ParseFailed("[default_address] Key bytes parsing failed"))
        };
        let public_key: PublicKey = deserialize(key_bytes)?;

        Ok(public_key)
    }

    /// Set provided index address as default in the wallet.
    pub async fn set_default_address(&self, idx: usize) -> WalletDbResult<()> {
        // First we update previous default record
        let is_default = 0;
        let query = format!("UPDATE {} SET {} = ?1", *MONEY_KEYS_TABLE, MONEY_KEYS_COL_IS_DEFAULT,);
        self.wallet.exec_sql(&query, rusqlite::params![is_default]).await?;

        // and then we set the new one
        let is_default = 1;
        let query = format!(
            "UPDATE {} SET {} = ?1 WHERE {} = ?2",
            *MONEY_KEYS_TABLE, MONEY_KEYS_COL_IS_DEFAULT, MONEY_KEYS_COL_KEY_ID,
        );
        self.wallet.exec_sql(&query, rusqlite::params![is_default, idx]).await
    }

    /// Fetch all pukeys from the wallet.
    pub async fn addresses(&self) -> Result<Vec<(u64, PublicKey, SecretKey, u64)>> {
        let rows = match self.wallet.query_multiple(&MONEY_KEYS_TABLE, &[], &[]).await {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[addresses] Addresses retrieval failed: {e:?}"
                )))
            }
        };

        let mut vec = Vec::with_capacity(rows.len());
        for row in rows {
            let Value::Integer(key_id) = row[0] else {
                return Err(Error::ParseFailed("[addresses] Key ID parsing failed"))
            };
            let Ok(key_id) = u64::try_from(key_id) else {
                return Err(Error::ParseFailed("[addresses] Key ID parsing failed"))
            };

            let Value::Integer(is_default) = row[1] else {
                return Err(Error::ParseFailed("[addresses] Is default parsing failed"))
            };
            let Ok(is_default) = u64::try_from(is_default) else {
                return Err(Error::ParseFailed("[addresses] Is default parsing failed"))
            };

            let Value::Blob(ref key_bytes) = row[2] else {
                return Err(Error::ParseFailed("[addresses] Public key bytes parsing failed"))
            };
            let public_key: PublicKey = deserialize(key_bytes)?;

            let Value::Blob(ref key_bytes) = row[3] else {
                return Err(Error::ParseFailed("[addresses] Secret key bytes parsing failed"))
            };
            let secret_key: SecretKey = deserialize(key_bytes)?;

            vec.push((key_id, public_key, secret_key, is_default));
        }

        Ok(vec)
    }

    /// Fetch all secret keys from the wallet.
    pub async fn get_money_secrets(&self) -> Result<Vec<SecretKey>> {
        let rows = match self
            .wallet
            .query_multiple(&MONEY_KEYS_TABLE, &[MONEY_KEYS_COL_SECRET], &[])
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_money_secrets] Secret keys retrieval failed: {e:?}"
                )))
            }
        };

        let mut secrets = Vec::with_capacity(rows.len());

        // Let's scan through the rows and see if we got anything.
        for row in rows {
            let Value::Blob(ref key_bytes) = row[0] else {
                return Err(Error::ParseFailed(
                    "[get_money_secrets] Secret key bytes parsing failed",
                ))
            };
            let secret_key: SecretKey = deserialize(key_bytes)?;
            secrets.push(secret_key);
        }

        Ok(secrets)
    }

    /// Import given secret keys into the wallet.
    /// If the key already exists, it will be skipped.
    /// Returns the respective PublicKey objects for the imported keys.
    pub async fn import_money_secrets(&self, secrets: Vec<SecretKey>) -> Result<Vec<PublicKey>> {
        let existing_secrets = self.get_money_secrets().await?;

        let mut ret = Vec::with_capacity(secrets.len());

        for secret in secrets {
            // Check if secret already exists
            if existing_secrets.contains(&secret) {
                eprintln!("Existing key found: {secret}");
                continue
            }

            ret.push(PublicKey::from_secret(secret));
            let is_default = 0;
            let public = serialize(&PublicKey::from_secret(secret));
            let secret = serialize(&secret);

            let query = format!(
                "INSERT INTO {} ({}, {}, {}) VALUES (?1, ?2, ?3);",
                *MONEY_KEYS_TABLE,
                MONEY_KEYS_COL_IS_DEFAULT,
                MONEY_KEYS_COL_PUBLIC,
                MONEY_KEYS_COL_SECRET
            );
            if let Err(e) =
                self.wallet.exec_sql(&query, rusqlite::params![is_default, public, secret]).await
            {
                return Err(Error::RusqliteError(format!(
                    "[import_money_secrets] Inserting new address failed: {e:?}"
                )))
            }
        }

        Ok(ret)
    }

    /// Fetch known unspent balances from the wallet and return them as a hashmap.
    pub async fn money_balance(&self) -> Result<HashMap<String, u64>> {
        let mut coins = self.get_coins(false).await?;
        coins.retain(|x| x.0.note.spend_hook == FuncId::none());

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

    /// Fetch all coins and their metadata related to the Money contract from the wallet.
    /// Optionally also fetch spent ones.
    /// The boolean in the returned tuple notes if the coin was marked as spent.
    pub async fn get_coins(&self, fetch_spent: bool) -> Result<Vec<(OwnCoin, bool)>> {
        let query = if fetch_spent {
            self.wallet.query_multiple(&MONEY_COINS_TABLE, &[], &[]).await
        } else {
            self.wallet
                .query_multiple(
                    &MONEY_COINS_TABLE,
                    &[],
                    convert_named_params! {(MONEY_COINS_COL_IS_SPENT, false)},
                )
                .await
        };

        let rows = match query {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_coins] Coins retrieval failed: {e:?}"
                )))
            }
        };

        let mut owncoins = Vec::with_capacity(rows.len());

        for row in rows {
            let Value::Blob(ref coin_bytes) = row[0] else {
                return Err(Error::ParseFailed("[get_coins] Coin bytes parsing failed"))
            };
            let coin: Coin = deserialize(coin_bytes)?;

            let Value::Integer(is_spent) = row[1] else {
                return Err(Error::ParseFailed("[get_coins] Is spent parsing failed"))
            };
            let Ok(is_spent) = u64::try_from(is_spent) else {
                return Err(Error::ParseFailed("[get_coins] Is spent parsing failed"))
            };
            let is_spent = is_spent > 0;

            let Value::Blob(ref value_bytes) = row[2] else {
                return Err(Error::ParseFailed("[get_coins] Value bytes parsing failed"))
            };
            let value: u64 = deserialize(value_bytes)?;

            let Value::Blob(ref token_id_bytes) = row[3] else {
                return Err(Error::ParseFailed("[get_coins] Token ID bytes parsing failed"))
            };
            let token_id: TokenId = deserialize(token_id_bytes)?;

            let Value::Blob(ref spend_hook_bytes) = row[4] else {
                return Err(Error::ParseFailed("[get_coins] Spend hook bytes parsing failed"))
            };
            let spend_hook: pallas::Base = deserialize(spend_hook_bytes)?;

            let Value::Blob(ref user_data_bytes) = row[5] else {
                return Err(Error::ParseFailed("[get_coins] User data bytes parsing failed"))
            };
            let user_data: pallas::Base = deserialize(user_data_bytes)?;

            let Value::Blob(ref coin_blind_bytes) = row[6] else {
                return Err(Error::ParseFailed("[get_coins] Coin blind bytes parsing failed"))
            };
            let coin_blind: pallas::Base = deserialize(coin_blind_bytes)?;

            let Value::Blob(ref value_blind_bytes) = row[7] else {
                return Err(Error::ParseFailed("[get_coins] Value blind bytes parsing failed"))
            };
            let value_blind: pallas::Scalar = deserialize(value_blind_bytes)?;

            let Value::Blob(ref token_blind_bytes) = row[8] else {
                return Err(Error::ParseFailed("[get_coins] Token blind bytes parsing failed"))
            };
            let token_blind: pallas::Base = deserialize(token_blind_bytes)?;

            let Value::Blob(ref secret_bytes) = row[9] else {
                return Err(Error::ParseFailed("[get_coins] Secret bytes parsing failed"))
            };
            let secret: SecretKey = deserialize(secret_bytes)?;

            let Value::Blob(ref nullifier_bytes) = row[10] else {
                return Err(Error::ParseFailed("[get_coins] Nullifier bytes parsing failed"))
            };
            let nullifier: Nullifier = deserialize(nullifier_bytes)?;

            let Value::Blob(ref leaf_position_bytes) = row[11] else {
                return Err(Error::ParseFailed("[get_coins] Leaf position bytes parsing failed"))
            };
            let leaf_position: bridgetree::Position = deserialize(leaf_position_bytes)?;

            let Value::Blob(ref memo) = row[12] else {
                return Err(Error::ParseFailed("[get_coins] Memo parsing failed"))
            };

            let note = MoneyNote {
                value,
                token_id,
                spend_hook: spend_hook.into(),
                user_data,
                coin_blind,
                value_blind,
                token_blind,
                memo: memo.clone(),
            };
            let owncoin = OwnCoin { coin, note, secret, nullifier, leaf_position };

            owncoins.push((owncoin, is_spent))
        }

        Ok(owncoins)
    }

    /// Create an alias record for provided Token ID.
    pub async fn add_alias(&self, alias: String, token_id: TokenId) -> WalletDbResult<()> {
        eprintln!("Generating alias {alias} for Token: {token_id}");
        let query = format!(
            "INSERT OR REPLACE INTO {} ({}, {}) VALUES (?1, ?2);",
            *MONEY_ALIASES_TABLE, MONEY_ALIASES_COL_ALIAS, MONEY_ALIASES_COL_TOKEN_ID,
        );
        self.wallet
            .exec_sql(&query, rusqlite::params![serialize(&alias), serialize(&token_id)])
            .await
    }

    /// Fetch all aliases from the wallet.
    /// Optionally filter using alias name and/or token id.
    pub async fn get_aliases(
        &self,
        alias_filter: Option<String>,
        token_id_filter: Option<TokenId>,
    ) -> Result<HashMap<String, TokenId>> {
        let rows = match self.wallet.query_multiple(&MONEY_ALIASES_TABLE, &[], &[]).await {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_aliases] Aliases retrieval failed: {e:?}"
                )))
            }
        };

        // Fill this map with aliases
        let mut map: HashMap<String, TokenId> = HashMap::new();
        for row in rows {
            let Value::Blob(ref alias_bytes) = row[0] else {
                return Err(Error::ParseFailed("[get_aliases] Alias bytes parsing failed"))
            };
            let alias: String = deserialize(alias_bytes)?;
            if alias_filter.is_some() && alias_filter.as_ref().unwrap() != &alias {
                continue
            }

            let Value::Blob(ref token_id_bytes) = row[1] else {
                return Err(Error::ParseFailed("[get_aliases] TokenId bytes parsing failed"))
            };
            let token_id: TokenId = deserialize(token_id_bytes)?;
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

    /// Remove provided alias record from the wallet database.
    pub async fn remove_alias(&self, alias: String) -> WalletDbResult<()> {
        eprintln!("Removing alias: {alias}");
        let query = format!(
            "DELETE FROM {} WHERE {} = ?1;",
            *MONEY_ALIASES_TABLE, MONEY_ALIASES_COL_ALIAS,
        );
        self.wallet.exec_sql(&query, rusqlite::params![serialize(&alias)]).await
    }

    /// Mark a given coin in the wallet as unspent.
    pub async fn unspend_coin(&self, coin: &Coin) -> WalletDbResult<()> {
        let is_spend = 0;
        let query = format!(
            "UPDATE {} SET {} = ?1 WHERE {} = ?2",
            *MONEY_COINS_TABLE, MONEY_COINS_COL_IS_SPENT, MONEY_COINS_COL_COIN,
        );
        self.wallet.exec_sql(&query, rusqlite::params![is_spend, serialize(&coin.inner())]).await
    }

    /// Replace the Money Merkle tree in the wallet.
    pub async fn put_money_tree(&self, tree: &MerkleTree) -> WalletDbResult<()> {
        // First we remove old record
        let query = format!("DELETE FROM {};", *MONEY_TREE_TABLE);
        self.wallet.exec_sql(&query, &[]).await?;

        // then we insert the new one
        let query =
            format!("INSERT INTO {} ({}) VALUES (?1);", *MONEY_TREE_TABLE, MONEY_TREE_COL_TREE,);
        self.wallet.exec_sql(&query, rusqlite::params![serialize(tree)]).await
    }

    /// Fetch the Money Merkle tree from the wallet.
    pub async fn get_money_tree(&self) -> Result<MerkleTree> {
        let row =
            match self.wallet.query_single(&MONEY_TREE_TABLE, &[MONEY_TREE_COL_TREE], &[]).await {
                Ok(r) => r,
                Err(e) => {
                    return Err(Error::RusqliteError(format!(
                        "[get_money_tree] Tree retrieval failed: {e:?}"
                    )))
                }
            };

        let Value::Blob(ref tree_bytes) = row[0] else {
            return Err(Error::ParseFailed("[get_money_tree] Tree bytes parsing failed"))
        };
        let tree = deserialize(tree_bytes)?;
        Ok(tree)
    }

    /// Get the last scanned slot from the wallet.
    pub async fn last_scanned_slot(&self) -> WalletDbResult<u64> {
        let ret = self
            .wallet
            .query_single(&MONEY_INFO_TABLE, &[MONEY_INFO_COL_LAST_SCANNED_SLOT], &[])
            .await?;
        let Value::Integer(slot) = ret[0] else {
            return Err(WalletDbError::ParseColumnValueError);
        };
        let Ok(slot) = u64::try_from(slot) else {
            return Err(WalletDbError::ParseColumnValueError);
        };

        Ok(slot)
    }

    /// Append data related to Money contract transactions into the wallet database.
    pub async fn apply_tx_money_data(&self, tx: &Transaction, _confirm: bool) -> Result<()> {
        let cid = *MONEY_CONTRACT_ID;

        let mut nullifiers: Vec<Nullifier> = vec![];
        let mut coins: Vec<Coin> = vec![];
        let mut notes: Vec<AeadEncryptedNote> = vec![];
        let mut freezes: Vec<TokenId> = vec![];

        for (i, call) in tx.calls.iter().enumerate() {
            if call.data.contract_id == cid && call.data.data[0] == MoneyFunction::TransferV1 as u8
            {
                eprintln!("Found Money::TransferV1 in call {i}");
                let params: MoneyTransferParamsV1 = deserialize(&call.data.data[1..])?;

                for input in params.inputs {
                    nullifiers.push(input.nullifier);
                }

                for output in params.outputs {
                    coins.push(output.coin);
                    notes.push(output.note);
                }

                continue
            }

            if call.data.contract_id == cid && call.data.data[0] == MoneyFunction::OtcSwapV1 as u8 {
                eprintln!("Found Money::OtcSwapV1 in call {i}");
                let params: MoneyTransferParamsV1 = deserialize(&call.data.data[1..])?;

                for input in params.inputs {
                    nullifiers.push(input.nullifier);
                }

                for output in params.outputs {
                    coins.push(output.coin);
                    notes.push(output.note);
                }

                continue
            }

            if call.data.contract_id == cid && call.data.data[0] == MoneyFunction::TokenMintV1 as u8
            {
                eprintln!("Found Money::MintV1 in call {i}");
                let params: MoneyTokenMintParamsV1 = deserialize(&call.data.data[1..])?;
                coins.push(params.coin);
                //notes.push(output.note);
                continue
            }

            if call.data.contract_id == cid &&
                call.data.data[0] == MoneyFunction::TokenFreezeV1 as u8
            {
                eprintln!("Found Money::FreezeV1 in call {i}");
                let params: MoneyTokenFreezeParamsV1 = deserialize(&call.data.data[1..])?;
                let token_id = TokenId::derive_public(params.mint_public);
                freezes.push(token_id);
            }
        }

        let secrets = self.get_money_secrets().await?;
        let dao_secrets = self.get_dao_secrets().await?;
        let mut tree = self.get_money_tree().await?;

        let mut owncoins = vec![];

        for (coin, note) in coins.iter().zip(notes.iter()) {
            // Append the new coin to the Merkle tree. Every coin has to be added.
            tree.append(MerkleNode::from(coin.inner()));

            // Attempt to decrypt the note
            for secret in secrets.iter().chain(dao_secrets.iter()) {
                if let Ok(note) = note.decrypt::<MoneyNote>(secret) {
                    eprintln!("Successfully decrypted a Money Note");
                    eprintln!("Witnessing coin in Merkle tree");
                    let leaf_position = tree.mark().unwrap();

                    let owncoin = OwnCoin {
                        coin: coin.clone(),
                        note: note.clone(),
                        secret: *secret,
                        nullifier: Nullifier::from(poseidon_hash([secret.inner(), coin.inner()])),
                        leaf_position,
                    };

                    owncoins.push(owncoin);
                }
            }
        }

        if let Err(e) = self.put_money_tree(&tree).await {
            return Err(Error::RusqliteError(format!(
                "[apply_tx_money_data] Put Money tree failed: {e:?}"
            )))
        }
        if !nullifiers.is_empty() {
            self.mark_spent_coins(&nullifiers).await?;
        }

        // This is the SQL query we'll be executing to insert new coins
        // into the wallet
        let query = format!(
            "INSERT INTO {} ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13);",
            *MONEY_COINS_TABLE,
            MONEY_COINS_COL_COIN,
            MONEY_COINS_COL_IS_SPENT,
            MONEY_COINS_COL_SERIAL,
            MONEY_COINS_COL_VALUE,
            MONEY_COINS_COL_TOKEN_ID,
            MONEY_COINS_COL_SPEND_HOOK,
            MONEY_COINS_COL_USER_DATA,
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
            let params = rusqlite::params![
                serialize(&owncoin.coin),
                0, // <-- is_spent
                serialize(&owncoin.note.coin_blind),
                serialize(&owncoin.note.value),
                serialize(&owncoin.note.token_id),
                serialize(&owncoin.note.spend_hook),
                serialize(&owncoin.note.user_data),
                serialize(&owncoin.note.value_blind),
                serialize(&owncoin.note.token_blind),
                serialize(&owncoin.secret),
                serialize(&owncoin.nullifier),
                serialize(&owncoin.leaf_position),
                serialize(&owncoin.note.memo),
            ];

            if let Err(e) = self.wallet.exec_sql(&query, params).await {
                return Err(Error::RusqliteError(format!(
                    "[apply_tx_money_data] Inserting Money coin failed: {e:?}"
                )))
            }
        }

        for token_id in freezes {
            let query = format!(
                "UPDATE {} SET {} = 1 WHERE {} = ?1;",
                *MONEY_TOKENS_TABLE, MONEY_TOKENS_COL_IS_FROZEN, MONEY_TOKENS_COL_TOKEN_ID,
            );

            if let Err(e) =
                self.wallet.exec_sql(&query, rusqlite::params![serialize(&token_id)]).await
            {
                return Err(Error::RusqliteError(format!(
                    "[apply_tx_money_data] Inserting Money coin failed: {e:?}"
                )))
            }
        }

        if !owncoins.is_empty() {
            kaching().await;
        }

        Ok(())
    }

    /// Mark a coin in the wallet as spent
    pub async fn mark_spent_coin(&self, coin: &Coin) -> WalletDbResult<()> {
        let query = format!(
            "UPDATE {} SET {} = ?1 WHERE {} = ?2;",
            *MONEY_COINS_TABLE, MONEY_COINS_COL_IS_SPENT, MONEY_COINS_COL_COIN
        );
        let is_spent = 1;
        self.wallet.exec_sql(&query, rusqlite::params![is_spent, serialize(&coin.inner())]).await
    }

    /// Marks all coins in the wallet as spent, if their nullifier is in the given set
    pub async fn mark_spent_coins(&self, nullifiers: &[Nullifier]) -> Result<()> {
        if nullifiers.is_empty() {
            return Ok(())
        }

        for (coin, _) in self.get_coins(false).await? {
            if nullifiers.contains(&coin.nullifier) {
                if let Err(e) = self.mark_spent_coin(&coin.coin).await {
                    return Err(Error::RusqliteError(format!(
                        "[mark_spent_coins] Marking spent coin failed: {e:?}"
                    )))
                }
            }
        }

        Ok(())
    }

    /// Reset the Money Merkle tree in the wallet
    pub async fn reset_money_tree(&self) -> WalletDbResult<()> {
        eprintln!("Resetting Money Merkle tree");
        let mut tree = MerkleTree::new(100);
        tree.append(MerkleNode::from(pallas::Base::ZERO));
        let _ = tree.mark().unwrap();
        self.put_money_tree(&tree).await?;
        eprintln!("Successfully reset Money Merkle tree");

        Ok(())
    }

    /// Reset the Money coins in the wallet
    pub async fn reset_money_coins(&self) -> WalletDbResult<()> {
        eprintln!("Resetting coins");
        let query = format!("DELETE FROM {};", *MONEY_COINS_TABLE);
        self.wallet.exec_sql(&query, &[]).await?;
        eprintln!("Successfully reset coins");

        Ok(())
    }

    /// Retrieve token by provided string.
    /// Input string represents either an alias or a token id.
    pub async fn get_token(&self, input: String) -> Result<TokenId> {
        // Check if input is an alias(max 5 characters)
        if input.chars().count() <= 5 {
            let aliases = self.get_aliases(Some(input.clone()), None).await?;
            if let Some(token_id) = aliases.get(&input) {
                return Ok(*token_id)
            }
        }
        // Else parse input
        Ok(TokenId::from_str(input.as_str())?)
    }
}
