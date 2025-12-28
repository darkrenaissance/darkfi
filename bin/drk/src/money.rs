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

use std::{
    collections::{BTreeMap, HashMap},
    str::FromStr,
};

use lazy_static::lazy_static;
use rand::rngs::OsRng;
use rusqlite::types::Value;

use darkfi::{
    tx::Transaction,
    util::encoding::base64,
    validator::fees::compute_fee,
    zk::{halo2::Field, proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses, Proof},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_money_contract::{
    client::{
        compute_remainder_blind,
        fee_v1::{create_fee_proof, FeeCallInput, FeeCallOutput, FEE_CALL_GAS},
        MoneyNote, OwnCoin,
    },
    model::{
        Coin, Input, MoneyAuthTokenFreezeParamsV1, MoneyAuthTokenMintParamsV1, MoneyFeeParamsV1,
        MoneyGenesisMintParamsV1, MoneyPoWRewardParamsV1, MoneyTokenMintParamsV1,
        MoneyTransferParamsV1, Nullifier, Output, TokenId, DARK_TOKEN_ID,
    },
    MoneyFunction, MONEY_CONTRACT_ZKAS_FEE_NS_V1,
};
use darkfi_sdk::{
    bridgetree::Position,
    crypto::{
        keypair::{Address, Keypair, PublicKey, SecretKey, StandardAddress},
        note::AeadEncryptedNote,
        pasta_prelude::PrimeField,
        BaseBlind, FuncId, MerkleNode, MerkleTree, ScalarBlind, MONEY_CONTRACT_ID,
    },
    dark_tree::DarkLeaf,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, deserialize_async, serialize, serialize_async, AsyncEncodable};

use crate::{
    cache::CacheSmt,
    cli_util::kaching,
    convert_named_params,
    error::{WalletDbError, WalletDbResult},
    rpc::ScanCache,
    Drk,
};

// Money Merkle tree Sled key
pub const SLED_MERKLE_TREES_MONEY: &[u8] = b"_money_tree";

// Wallet SQL table constant names. These have to represent the `money.sql`
// SQL schema. Table names are prefixed with the contract ID to avoid collisions.
lazy_static! {
    pub static ref MONEY_KEYS_TABLE: String =
        format!("{}_money_keys", MONEY_CONTRACT_ID.to_string());
    pub static ref MONEY_COINS_TABLE: String =
        format!("{}_money_coins", MONEY_CONTRACT_ID.to_string());
    pub static ref MONEY_TOKENS_TABLE: String =
        format!("{}_money_tokens", MONEY_CONTRACT_ID.to_string());
    pub static ref MONEY_ALIASES_TABLE: String =
        format!("{}_money_aliases", MONEY_CONTRACT_ID.to_string());
}

// MONEY_KEYS_TABLE
pub const MONEY_KEYS_COL_KEY_ID: &str = "key_id";
pub const MONEY_KEYS_COL_IS_DEFAULT: &str = "is_default";
pub const MONEY_KEYS_COL_PUBLIC: &str = "public";
pub const MONEY_KEYS_COL_SECRET: &str = "secret";

// MONEY_COINS_TABLE
pub const MONEY_COINS_COL_COIN: &str = "coin";
pub const MONEY_COINS_COL_VALUE: &str = "value";
pub const MONEY_COINS_COL_TOKEN_ID: &str = "token_id";
pub const MONEY_COINS_COL_SPEND_HOOK: &str = "spend_hook";
pub const MONEY_COINS_COL_USER_DATA: &str = "user_data";
pub const MONEY_COINS_COL_COIN_BLIND: &str = "coin_blind";
pub const MONEY_COINS_COL_VALUE_BLIND: &str = "value_blind";
pub const MONEY_COINS_COL_TOKEN_BLIND: &str = "token_blind";
pub const MONEY_COINS_COL_SECRET: &str = "secret";
pub const MONEY_COINS_COL_LEAF_POSITION: &str = "leaf_position";
pub const MONEY_COINS_COL_MEMO: &str = "memo";
pub const MONEY_COINS_COL_CREATION_HEIGHT: &str = "creation_height";
pub const MONEY_COINS_COL_IS_SPENT: &str = "is_spent";
pub const MONEY_COINS_COL_SPENT_HEIGHT: &str = "spent_height";
pub const MONEY_COINS_COL_SPENT_TX_HASH: &str = "spent_tx_hash";

// MONEY_TOKENS_TABLE
pub const MONEY_TOKENS_COL_TOKEN_ID: &str = "token_id";
pub const MONEY_TOKENS_COL_MINT_AUTHORITY: &str = "mint_authority";
pub const MONEY_TOKENS_COL_TOKEN_BLIND: &str = "token_blind";
pub const MONEY_TOKENS_COL_IS_FROZEN: &str = "is_frozen";
pub const MONEY_TOKENS_COL_FREEZE_HEIGHT: &str = "freeze_height";

// MONEY_ALIASES_TABLE
pub const MONEY_ALIASES_COL_ALIAS: &str = "alias";
pub const MONEY_ALIASES_COL_TOKEN_ID: &str = "token_id";

pub const BALANCE_BASE10_DECIMALS: usize = 8;

impl Drk {
    /// Initialize wallet with tables for the Money contract.
    pub async fn initialize_money(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        // Initialize Money wallet schema
        let wallet_schema = include_str!("../money.sql");
        self.wallet.exec_batch_sql(wallet_schema)?;

        // Insert DRK alias
        self.add_alias("DRK".to_string(), *DARK_TOKEN_ID, output).await?;

        Ok(())
    }

    /// Generate a new keypair and place it into the wallet.
    pub async fn money_keygen(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Generating a new keypair"));

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
        self.wallet.exec_sql(
            &query,
            rusqlite::params![
                is_default,
                serialize_async(&keypair.public).await,
                serialize_async(&keypair.secret).await
            ],
        )?;

        output.push(String::from("New address:"));
        let address: Address = StandardAddress::from_public(self.network, keypair.public).into();
        output.push(format!("{address}"));

        Ok(())
    }

    /// Fetch default secret key from the wallet.
    pub async fn default_secret(&self) -> Result<SecretKey> {
        let row = match self.wallet.query_single(
            &MONEY_KEYS_TABLE,
            &[MONEY_KEYS_COL_SECRET],
            convert_named_params! {(MONEY_KEYS_COL_IS_DEFAULT, 1)},
        ) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[default_secret] Default secret key retrieval failed: {e}"
                )))
            }
        };

        let Value::Blob(ref key_bytes) = row[0] else {
            return Err(Error::ParseFailed("[default_secret] Key bytes parsing failed"))
        };
        let secret_key: SecretKey = deserialize_async(key_bytes).await?;

        Ok(secret_key)
    }

    /// Fetch default pubkey from the wallet.
    pub async fn default_address(&self) -> Result<PublicKey> {
        let row = match self.wallet.query_single(
            &MONEY_KEYS_TABLE,
            &[MONEY_KEYS_COL_PUBLIC],
            convert_named_params! {(MONEY_KEYS_COL_IS_DEFAULT, 1)},
        ) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[default_address] Default address retrieval failed: {e}"
                )))
            }
        };

        let Value::Blob(ref key_bytes) = row[0] else {
            return Err(Error::ParseFailed("[default_address] Key bytes parsing failed"))
        };
        let public_key: PublicKey = deserialize_async(key_bytes).await?;

        Ok(public_key)
    }

    /// Set provided index address as default in the wallet.
    pub fn set_default_address(&self, idx: usize) -> WalletDbResult<()> {
        // First we update previous default record
        let is_default = 0;
        let query = format!("UPDATE {} SET {} = ?1", *MONEY_KEYS_TABLE, MONEY_KEYS_COL_IS_DEFAULT,);
        self.wallet.exec_sql(&query, rusqlite::params![is_default])?;

        // and then we set the new one
        let is_default = 1;
        let query = format!(
            "UPDATE {} SET {} = ?1 WHERE {} = ?2",
            *MONEY_KEYS_TABLE, MONEY_KEYS_COL_IS_DEFAULT, MONEY_KEYS_COL_KEY_ID,
        );
        self.wallet.exec_sql(&query, rusqlite::params![is_default, idx])
    }

    /// Fetch all pukeys from the wallet.
    pub async fn addresses(&self) -> Result<Vec<(u64, PublicKey, SecretKey, u64)>> {
        let rows = match self.wallet.query_multiple(&MONEY_KEYS_TABLE, &[], &[]) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[addresses] Addresses retrieval failed: {e}"
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
            let public_key: PublicKey = deserialize_async(key_bytes).await?;

            let Value::Blob(ref key_bytes) = row[3] else {
                return Err(Error::ParseFailed("[addresses] Secret key bytes parsing failed"))
            };
            let secret_key: SecretKey = deserialize_async(key_bytes).await?;

            vec.push((key_id, public_key, secret_key, is_default));
        }

        Ok(vec)
    }

    /// Fetch provided index address from the wallet and generate its
    /// mining configuration.
    pub async fn mining_config(
        &self,
        idx: usize,
        spend_hook: Option<FuncId>,
        user_data: Option<pallas::Base>,
        output: &mut Vec<String>,
    ) -> Result<()> {
        let row = match self.wallet.query_single(
            &MONEY_KEYS_TABLE,
            &[MONEY_KEYS_COL_PUBLIC],
            convert_named_params! {(MONEY_KEYS_COL_KEY_ID, idx)},
        ) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[mining_address] Address retrieval failed: {e}"
                )))
            }
        };
        let Value::Blob(ref key_bytes) = row[0] else {
            return Err(Error::ParseFailed("[mining_address] Key bytes parsing failed"))
        };
        let public_key: PublicKey = deserialize_async(key_bytes).await?;
        let address: Address = StandardAddress::from_public(self.network, public_key).into();
        let recipient = address.to_string();
        let spend_hook = spend_hook.as_ref().map(|spend_hook| spend_hook.to_string());
        let user_data =
            user_data.as_ref().map(|user_data| bs58::encode(user_data.to_repr()).into_string());
        output.push(String::from("DarkFi mining configuration address:"));
        output.push(base64::encode(&serialize(&(recipient, spend_hook, user_data))).to_string());

        Ok(())
    }

    /// Fetch all secret keys from the wallet.
    pub async fn get_money_secrets(&self) -> Result<Vec<SecretKey>> {
        let rows =
            match self.wallet.query_multiple(&MONEY_KEYS_TABLE, &[MONEY_KEYS_COL_SECRET], &[]) {
                Ok(r) => r,
                Err(e) => {
                    return Err(Error::DatabaseError(format!(
                        "[get_money_secrets] Secret keys retrieval failed: {e}"
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
            let secret_key: SecretKey = deserialize_async(key_bytes).await?;
            secrets.push(secret_key);
        }

        Ok(secrets)
    }

    /// Import given secret keys into the wallet.
    /// If the key already exists, it will be skipped.
    /// Returns the respective PublicKey objects for the imported keys.
    pub async fn import_money_secrets(
        &self,
        secrets: Vec<SecretKey>,
        output: &mut Vec<String>,
    ) -> Result<Vec<PublicKey>> {
        let existing_secrets = self.get_money_secrets().await?;

        let mut ret = Vec::with_capacity(secrets.len());

        for secret in secrets {
            // Check if secret already exists
            if existing_secrets.contains(&secret) {
                output.push(format!("Existing key found: {secret}"));
                continue
            }

            ret.push(PublicKey::from_secret(secret));
            let is_default = 0;
            let public = serialize_async(&PublicKey::from_secret(secret)).await;
            let secret = serialize_async(&secret).await;

            let query = format!(
                "INSERT INTO {} ({}, {}, {}) VALUES (?1, ?2, ?3);",
                *MONEY_KEYS_TABLE,
                MONEY_KEYS_COL_IS_DEFAULT,
                MONEY_KEYS_COL_PUBLIC,
                MONEY_KEYS_COL_SECRET
            );
            if let Err(e) =
                self.wallet.exec_sql(&query, rusqlite::params![is_default, public, secret])
            {
                return Err(Error::DatabaseError(format!(
                    "[import_money_secrets] Inserting new address failed: {e}"
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
    /// The boolean in the returned tuple notes if the coin was marked
    /// as spent, along with the height and tx it was spent in.
    pub async fn get_coins(
        &self,
        fetch_spent: bool,
    ) -> Result<Vec<(OwnCoin, u32, bool, Option<u32>, String)>> {
        let query = if fetch_spent {
            self.wallet.query_multiple(&MONEY_COINS_TABLE, &[], &[])
        } else {
            self.wallet.query_multiple(
                &MONEY_COINS_TABLE,
                &[],
                convert_named_params! {(MONEY_COINS_COL_IS_SPENT, false)},
            )
        };

        let rows = match query {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::DatabaseError(format!("[get_coins] Coins retrieval failed: {e}")))
            }
        };

        let mut owncoins = Vec::with_capacity(rows.len());
        for row in rows {
            owncoins.push(self.parse_coin_record(&row).await?)
        }

        Ok(owncoins)
    }

    /// Fetch provided token unspend balances from the wallet.
    pub async fn get_token_coins(&self, token_id: &TokenId) -> Result<Vec<OwnCoin>> {
        let query = self.wallet.query_multiple(
            &MONEY_COINS_TABLE,
            &[],
            convert_named_params! {
                (MONEY_COINS_COL_TOKEN_ID, serialize_async(token_id).await),
                (MONEY_COINS_COL_SPEND_HOOK, serialize_async(&FuncId::none()).await),
                (MONEY_COINS_COL_IS_SPENT, false),
            },
        );

        let rows = match query {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[get_token_coins] Coins retrieval failed: {e}"
                )))
            }
        };

        let mut owncoins = Vec::with_capacity(rows.len());
        for row in rows {
            owncoins.push(self.parse_coin_record(&row).await?.0)
        }

        Ok(owncoins)
    }

    /// Fetch provided contract specified token unspend balances from the wallet.
    pub async fn get_contract_token_coins(
        &self,
        token_id: &TokenId,
        spend_hook: &FuncId,
        user_data: &pallas::Base,
    ) -> Result<Vec<OwnCoin>> {
        let query = self.wallet.query_multiple(
            &MONEY_COINS_TABLE,
            &[],
            convert_named_params! {
                (MONEY_COINS_COL_TOKEN_ID, serialize_async(token_id).await),
                (MONEY_COINS_COL_SPEND_HOOK, serialize_async(spend_hook).await),
                (MONEY_COINS_COL_USER_DATA, serialize_async(user_data).await),
                (MONEY_COINS_COL_IS_SPENT, false),
            },
        );

        let rows = match query {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[get_contract_token_coins] Coins retrieval failed: {e}"
                )))
            }
        };

        let mut owncoins = Vec::with_capacity(rows.len());
        for row in rows {
            owncoins.push(self.parse_coin_record(&row).await?.0)
        }

        Ok(owncoins)
    }

    /// Auxiliary function to parse a `MONEY_COINS_TABLE` record.
    /// The boolean in the returned tuple notes if the coin was marked
    /// as spent, along with the height and tx it was spent in.
    async fn parse_coin_record(
        &self,
        row: &[Value],
    ) -> Result<(OwnCoin, u32, bool, Option<u32>, String)> {
        let Value::Blob(ref coin_bytes) = row[0] else {
            return Err(Error::ParseFailed("[parse_coin_record] Coin bytes parsing failed"))
        };
        let coin: Coin = deserialize_async(coin_bytes).await?;

        let Value::Blob(ref value_bytes) = row[1] else {
            return Err(Error::ParseFailed("[parse_coin_record] Value bytes parsing failed"))
        };
        let value: u64 = deserialize_async(value_bytes).await?;

        let Value::Blob(ref token_id_bytes) = row[2] else {
            return Err(Error::ParseFailed("[parse_coin_record] Token ID bytes parsing failed"))
        };
        let token_id: TokenId = deserialize_async(token_id_bytes).await?;

        let Value::Blob(ref spend_hook_bytes) = row[3] else {
            return Err(Error::ParseFailed("[parse_coin_record] Spend hook bytes parsing failed"))
        };
        let spend_hook: pallas::Base = deserialize_async(spend_hook_bytes).await?;

        let Value::Blob(ref user_data_bytes) = row[4] else {
            return Err(Error::ParseFailed("[parse_coin_record] User data bytes parsing failed"))
        };
        let user_data: pallas::Base = deserialize_async(user_data_bytes).await?;

        let Value::Blob(ref coin_blind_bytes) = row[5] else {
            return Err(Error::ParseFailed("[parse_coin_record] Coin blind bytes parsing failed"))
        };
        let coin_blind: BaseBlind = deserialize_async(coin_blind_bytes).await?;

        let Value::Blob(ref value_blind_bytes) = row[6] else {
            return Err(Error::ParseFailed("[parse_coin_record] Value blind bytes parsing failed"))
        };
        let value_blind: ScalarBlind = deserialize_async(value_blind_bytes).await?;

        let Value::Blob(ref token_blind_bytes) = row[7] else {
            return Err(Error::ParseFailed("[parse_coin_record] Token blind bytes parsing failed"))
        };
        let token_blind: BaseBlind = deserialize_async(token_blind_bytes).await?;

        let Value::Blob(ref secret_bytes) = row[8] else {
            return Err(Error::ParseFailed("[parse_coin_record] Secret bytes parsing failed"))
        };
        let secret: SecretKey = deserialize_async(secret_bytes).await?;

        let Value::Blob(ref leaf_position_bytes) = row[9] else {
            return Err(Error::ParseFailed("[parse_coin_record] Leaf position bytes parsing failed"))
        };
        let leaf_position: Position = deserialize_async(leaf_position_bytes).await?;

        let Value::Blob(ref memo) = row[10] else {
            return Err(Error::ParseFailed("[parse_coin_record] Memo parsing failed"))
        };

        let Value::Integer(creation_height) = row[11] else {
            return Err(Error::ParseFailed("[parse_coin_record] Creation height parsing failed"))
        };
        let Ok(creation_height) = u32::try_from(creation_height) else {
            return Err(Error::ParseFailed("[parse_coin_record] Creation height parsing failed"))
        };

        let Value::Integer(is_spent) = row[12] else {
            return Err(Error::ParseFailed("[parse_coin_record] Is spent parsing failed"))
        };
        let Ok(is_spent) = u64::try_from(is_spent) else {
            return Err(Error::ParseFailed("[parse_coin_record] Is spent parsing failed"))
        };
        let is_spent = is_spent > 0;

        let spent_height = match row[13] {
            Value::Integer(spent_height) => {
                let Ok(spent_height) = u32::try_from(spent_height) else {
                    return Err(Error::ParseFailed(
                        "[parse_coin_record] Spent height parsing failed",
                    ))
                };
                Some(spent_height)
            }
            Value::Null => None,
            _ => return Err(Error::ParseFailed("[parse_coin_record] Spent height parsing failed")),
        };

        let Value::Text(ref spent_tx_hash) = row[14] else {
            return Err(Error::ParseFailed(
                "[parse_coin_record] Spent transaction hash parsing failed",
            ))
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

        Ok((
            OwnCoin { coin, note, secret, leaf_position },
            creation_height,
            is_spent,
            spent_height,
            spent_tx_hash.clone(),
        ))
    }

    /// Create an alias record for provided Token ID.
    pub async fn add_alias(
        &self,
        alias: String,
        token_id: TokenId,
        output: &mut Vec<String>,
    ) -> WalletDbResult<()> {
        output.push(format!("Generating alias {alias} for Token: {token_id}"));
        let query = format!(
            "INSERT OR REPLACE INTO {} ({}, {}) VALUES (?1, ?2);",
            *MONEY_ALIASES_TABLE, MONEY_ALIASES_COL_ALIAS, MONEY_ALIASES_COL_TOKEN_ID,
        );
        self.wallet.exec_sql(
            &query,
            rusqlite::params![serialize_async(&alias).await, serialize_async(&token_id).await],
        )
    }

    /// Fetch all aliases from the wallet.
    /// Optionally filter using alias name and/or token id.
    pub async fn get_aliases(
        &self,
        alias_filter: Option<String>,
        token_id_filter: Option<TokenId>,
    ) -> Result<HashMap<String, TokenId>> {
        let rows = match self.wallet.query_multiple(&MONEY_ALIASES_TABLE, &[], &[]) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[get_aliases] Aliases retrieval failed: {e}"
                )))
            }
        };

        // Fill this map with aliases
        let mut map: HashMap<String, TokenId> = HashMap::new();
        for row in rows {
            let Value::Blob(ref alias_bytes) = row[0] else {
                return Err(Error::ParseFailed("[get_aliases] Alias bytes parsing failed"))
            };
            let alias: String = deserialize_async(alias_bytes).await?;
            if alias_filter.is_some() && alias_filter.as_ref().unwrap() != &alias {
                continue
            }

            let Value::Blob(ref token_id_bytes) = row[1] else {
                return Err(Error::ParseFailed("[get_aliases] TokenId bytes parsing failed"))
            };
            let token_id: TokenId = deserialize_async(token_id_bytes).await?;
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
                format!("{prev}, {alias}")
            } else {
                alias
            };

            map.insert(token_id.to_string(), aliases_string);
        }

        Ok(map)
    }

    /// Remove provided alias record from the wallet database.
    pub async fn remove_alias(
        &self,
        alias: String,
        output: &mut Vec<String>,
    ) -> WalletDbResult<()> {
        output.push(format!("Removing alias: {alias}"));
        let query = format!(
            "DELETE FROM {} WHERE {} = ?1;",
            *MONEY_ALIASES_TABLE, MONEY_ALIASES_COL_ALIAS,
        );
        self.wallet.exec_sql(&query, rusqlite::params![serialize_async(&alias).await])
    }

    /// Mark a given coin in the wallet as unspent.
    pub async fn unspend_coin(&self, coin: &Coin) -> WalletDbResult<()> {
        let query = format!(
            "UPDATE {} SET {} = 0, {} = NULL, {} = '-' WHERE {} = ?1;",
            *MONEY_COINS_TABLE,
            MONEY_COINS_COL_IS_SPENT,
            MONEY_COINS_COL_SPENT_HEIGHT,
            MONEY_COINS_COL_SPENT_TX_HASH,
            MONEY_COINS_COL_COIN
        );
        self.wallet.exec_sql(&query, rusqlite::params![serialize_async(&coin.inner()).await])
    }

    /// Fetch the Money Merkle tree from the cache.
    /// If it doesn't exists a new Merkle Tree is returned.
    pub async fn get_money_tree(&self) -> Result<MerkleTree> {
        match self.cache.merkle_trees.get(SLED_MERKLE_TREES_MONEY)? {
            Some(tree_bytes) => Ok(deserialize_async(&tree_bytes).await?),
            None => {
                let mut tree = MerkleTree::new(u32::MAX as usize);
                tree.append(MerkleNode::from(pallas::Base::ZERO));
                let _ = tree.mark().unwrap();
                Ok(tree)
            }
        }
    }

    /// Auxiliary function to grab all the nullifiers, coins with their
    /// notes and a flag indicating if its a block reward, and freezes
    /// from a transaction money call.
    async fn parse_money_call(
        &self,
        scan_cache: &mut ScanCache,
        call_idx: &usize,
        calls: &[DarkLeaf<ContractCall>],
    ) -> Result<(Vec<Nullifier>, Vec<(Coin, AeadEncryptedNote, bool)>, Vec<TokenId>)> {
        let mut nullifiers: Vec<Nullifier> = vec![];
        let mut coins: Vec<(Coin, AeadEncryptedNote, bool)> = vec![];
        let mut freezes: Vec<TokenId> = vec![];

        let call = &calls[*call_idx];
        let data = &call.data.data;
        match MoneyFunction::try_from(data[0])? {
            MoneyFunction::FeeV1 => {
                scan_cache.log(String::from("[parse_money_call] Found Money::FeeV1 call"));
                let params: MoneyFeeParamsV1 = deserialize_async(&data[9..]).await?;
                nullifiers.push(params.input.nullifier);
                coins.push((params.output.coin, params.output.note, false));
            }
            MoneyFunction::GenesisMintV1 => {
                scan_cache.log(String::from("[parse_money_call] Found Money::GenesisMintV1 call"));
                let params: MoneyGenesisMintParamsV1 = deserialize_async(&data[1..]).await?;
                for output in params.outputs {
                    coins.push((output.coin, output.note, false));
                }
            }
            MoneyFunction::PoWRewardV1 => {
                scan_cache.log(String::from("[parse_money_call] Found Money::PoWRewardV1 call"));
                let params: MoneyPoWRewardParamsV1 = deserialize_async(&data[1..]).await?;
                coins.push((params.output.coin, params.output.note, true));
            }
            MoneyFunction::TransferV1 => {
                scan_cache.log(String::from("[parse_money_call] Found Money::TransferV1 call"));
                let params: MoneyTransferParamsV1 = deserialize_async(&data[1..]).await?;

                for input in params.inputs {
                    nullifiers.push(input.nullifier);
                }

                for output in params.outputs {
                    coins.push((output.coin, output.note, false));
                }
            }
            MoneyFunction::OtcSwapV1 => {
                scan_cache.log(String::from("[parse_money_call] Found Money::OtcSwapV1 call"));
                let params: MoneyTransferParamsV1 = deserialize_async(&data[1..]).await?;

                for input in params.inputs {
                    nullifiers.push(input.nullifier);
                }

                for output in params.outputs {
                    coins.push((output.coin, output.note, false));
                }
            }
            MoneyFunction::AuthTokenMintV1 => {
                scan_cache
                    .log(String::from("[parse_money_call] Found Money::AuthTokenMintV1 call"));
                // Handled in TokenMint
            }
            MoneyFunction::AuthTokenFreezeV1 => {
                scan_cache
                    .log(String::from("[parse_money_call] Found Money::AuthTokenFreezeV1 call"));
                let params: MoneyAuthTokenFreezeParamsV1 = deserialize_async(&data[1..]).await?;
                freezes.push(params.token_id);
            }
            MoneyFunction::TokenMintV1 => {
                scan_cache.log(String::from("[parse_money_call] Found Money::TokenMintV1 call"));
                let params: MoneyTokenMintParamsV1 = deserialize_async(&data[1..]).await?;
                // Grab the note from the child auth call
                let child_idx = call.children_indexes[0];
                let child_call = &calls[child_idx];
                let child_params: MoneyAuthTokenMintParamsV1 =
                    deserialize_async(&child_call.data.data[1..]).await?;
                coins.push((params.coin, child_params.enc_note, false));
            }
        }

        Ok((nullifiers, coins, freezes))
    }

    /// Auxiliary function to handle coins with their notes and flag
    /// indicating if its a block reward from a transaction money call.
    /// Returns our found own coins along with the block signing key,
    /// if found.
    fn handle_money_call_coins(
        &self,
        tree: &mut MerkleTree,
        secrets: &[SecretKey],
        messages_buffer: &mut Vec<String>,
        coins: &[(Coin, AeadEncryptedNote, bool)],
    ) -> Result<(Vec<OwnCoin>, Option<SecretKey>)> {
        // Keep track of our own coins found in the vec
        let mut owncoins = vec![];

        // Check if provided coins vec is empty
        if coins.is_empty() {
            return Ok((owncoins, None))
        }

        // Handle provided coins vector and grab our own,
        // along with the block signing key if its a block
        // reward coin. Only one reward call and coin exists
        // in each block.
        let mut block_signing_key = None;
        for (coin, note, is_block_reward) in coins {
            // Append the new coin to the Merkle tree.
            // Every coin has to be added.
            tree.append(MerkleNode::from(coin.inner()));

            // Attempt to decrypt the note
            for secret in secrets {
                let Ok(note) = note.decrypt::<MoneyNote>(secret) else { continue };
                messages_buffer.push(String::from(
                    "[handle_money_call_coins] Successfully decrypted a Money Note",
                ));
                messages_buffer
                    .push(String::from("[handle_money_call_coins] Witnessing coin in Merkle tree"));
                let leaf_position = tree.mark().unwrap();
                if *is_block_reward {
                    messages_buffer
                        .push(String::from("[handle_money_call_coins] Grabing block signing key"));
                    block_signing_key = Some(deserialize(&note.memo)?);
                }
                let owncoin = OwnCoin { coin: *coin, note, secret: *secret, leaf_position };
                owncoins.push(owncoin);
                break
            }
        }

        Ok((owncoins, block_signing_key))
    }

    /// Auxiliary function to handle own coins from a transaction money
    /// call.
    async fn handle_money_call_owncoins(
        &self,
        scan_cache: &mut ScanCache,
        coins: &[OwnCoin],
        creation_height: &u32,
    ) -> Result<()> {
        scan_cache.log(format!("Found {} OwnCoin(s) in transaction", coins.len()));

        // Check if we have any owncoins to process
        if coins.is_empty() {
            return Ok(())
        }

        // This is the SQL query we'll be executing to insert new coins into the wallet
        let query = format!(
            "INSERT INTO {} ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14);",
            *MONEY_COINS_TABLE,
            MONEY_COINS_COL_COIN,
            MONEY_COINS_COL_VALUE,
            MONEY_COINS_COL_TOKEN_ID,
            MONEY_COINS_COL_SPEND_HOOK,
            MONEY_COINS_COL_USER_DATA,
            MONEY_COINS_COL_COIN_BLIND,
            MONEY_COINS_COL_VALUE_BLIND,
            MONEY_COINS_COL_TOKEN_BLIND,
            MONEY_COINS_COL_SECRET,
            MONEY_COINS_COL_LEAF_POSITION,
            MONEY_COINS_COL_MEMO,
            MONEY_COINS_COL_CREATION_HEIGHT,
            MONEY_COINS_COL_IS_SPENT,
            MONEY_COINS_COL_SPENT_HEIGHT,
        );

        // Handle our own coins
        let spent_height: Option<u32> = None;
        for coin in coins {
            scan_cache.log(format!("OwnCoin: {:?}", coin.coin));
            // Grab coin record key
            let key = coin.coin.to_bytes();

            // Push to our own coins nullifiers cache
            scan_cache
                .owncoins_nullifiers
                .insert(coin.nullifier().to_bytes(), (key, coin.leaf_position));

            // Execute the query
            let params = rusqlite::params![
                key,
                serialize(&coin.note.value),
                serialize(&coin.note.token_id),
                serialize(&coin.note.spend_hook),
                serialize(&coin.note.user_data),
                serialize(&coin.note.coin_blind),
                serialize(&coin.note.value_blind),
                serialize(&coin.note.token_blind),
                serialize(&coin.secret),
                serialize(&coin.leaf_position),
                serialize(&coin.note.memo),
                creation_height,
                0, // <-- is_spent
                spent_height,
            ];

            if let Err(e) = self.wallet.exec_sql(&query, params) {
                return Err(Error::DatabaseError(format!(
                    "[handle_money_call_owncoins] Inserting Money coin failed: {e}"
                )))
            }
        }

        Ok(())
    }

    /// Auxiliary function to handle freezes from a transaction money
    /// call.
    /// Returns a flag indicating if provided freezes refer to our own
    /// wallet.
    async fn handle_money_call_freezes(
        &self,
        own_tokens: &[TokenId],
        freezes: &[TokenId],
        freeze_height: &u32,
    ) -> Result<bool> {
        // Check if we have any freezes to process
        if freezes.is_empty() {
            return Ok(false)
        }

        // Find our own tokens that got frozen
        let mut own_freezes = Vec::with_capacity(freezes.len());
        for freeze in freezes {
            if own_tokens.contains(freeze) {
                own_freezes.push(freeze);
            }
        }

        // Check if we need to freeze anything
        if own_freezes.is_empty() {
            return Ok(false)
        }

        // This is the SQL query we'll be executing to update frozen tokens into the wallet
        let query = format!(
            "UPDATE {} SET {} = 1, {} = ?1 WHERE {} = ?2;",
            *MONEY_TOKENS_TABLE,
            MONEY_TOKENS_COL_IS_FROZEN,
            MONEY_TOKENS_COL_FREEZE_HEIGHT,
            MONEY_TOKENS_COL_TOKEN_ID,
        );

        for token_id in own_freezes {
            // Grab token record key
            let key = serialize_async(token_id).await;

            // Execute the query
            if let Err(e) =
                self.wallet.exec_sql(&query, rusqlite::params![Some(*freeze_height), key])
            {
                return Err(Error::DatabaseError(format!(
                    "[handle_money_call_freezes] Update Money token freeze failed: {e}"
                )))
            }
        }

        Ok(true)
    }

    /// Append data related to Money contract transactions into the
    /// wallet database and update the provided scan cache.
    /// Returns a flag indicating if provided data refer to our own
    /// wallet along with the block signing key, if found.
    pub async fn apply_tx_money_data(
        &self,
        scan_cache: &mut ScanCache,
        call_idx: &usize,
        calls: &[DarkLeaf<ContractCall>],
        tx_hash: &String,
        block_height: &u32,
    ) -> Result<(bool, Option<SecretKey>)> {
        // Parse the call
        let (nullifiers, coins, freezes) =
            self.parse_money_call(scan_cache, call_idx, calls).await?;

        // Parse call coins and grab our own
        let (owncoins, block_signing_key) = self.handle_money_call_coins(
            &mut scan_cache.money_tree,
            &scan_cache.notes_secrets,
            &mut scan_cache.messages_buffer,
            &coins,
        )?;

        // Update nullifiers smt
        self.smt_insert(&mut scan_cache.money_smt, &nullifiers)?;

        // Check if we have any spent coins
        let wallet_spent_coins = self.mark_spent_coins(
            Some(&mut scan_cache.money_tree),
            &scan_cache.owncoins_nullifiers,
            &nullifiers,
            &Some(*block_height),
            tx_hash,
        )?;

        // Handle our own coins
        self.handle_money_call_owncoins(scan_cache, &owncoins, block_height).await?;

        // Handle freezes
        let wallet_freezes =
            self.handle_money_call_freezes(&scan_cache.own_tokens, &freezes, block_height).await?;

        if self.fun && !owncoins.is_empty() {
            kaching().await;
        }

        Ok((wallet_spent_coins || !owncoins.is_empty() || wallet_freezes, block_signing_key))
    }

    /// Auxiliary function to  grab all the nullifiers from a transaction money call.
    async fn money_call_nullifiers(&self, call: &DarkLeaf<ContractCall>) -> Result<Vec<Nullifier>> {
        let mut nullifiers: Vec<Nullifier> = vec![];

        let data = &call.data.data;
        match MoneyFunction::try_from(data[0])? {
            MoneyFunction::FeeV1 => {
                let params: MoneyFeeParamsV1 = deserialize_async(&data[9..]).await?;
                nullifiers.push(params.input.nullifier);
            }
            MoneyFunction::TransferV1 => {
                let params: MoneyTransferParamsV1 = deserialize_async(&data[1..]).await?;

                for input in params.inputs {
                    nullifiers.push(input.nullifier);
                }
            }
            MoneyFunction::OtcSwapV1 => {
                let params: MoneyTransferParamsV1 = deserialize_async(&data[1..]).await?;

                for input in params.inputs {
                    nullifiers.push(input.nullifier);
                }
            }
            _ => { /* Do nothing */ }
        }

        Ok(nullifiers)
    }

    /// Mark provided transaction input coins as spent.
    pub async fn mark_tx_spend(&self, tx: &Transaction, output: &mut Vec<String>) -> Result<()> {
        // Create a cache of all our own nullifiers
        let mut owncoins_nullifiers = BTreeMap::new();
        for coin in self.get_coins(true).await? {
            owncoins_nullifiers.insert(
                coin.0.nullifier().to_bytes(),
                (coin.0.coin.to_bytes(), coin.0.leaf_position),
            );
        }

        let tx_hash = tx.hash().to_string();
        output.push(format!("[mark_tx_spend] Processing transaction: {tx_hash}"));
        for (i, call) in tx.calls.iter().enumerate() {
            if call.data.contract_id != *MONEY_CONTRACT_ID {
                continue
            }

            output.push(format!("[mark_tx_spend] Found Money contract in call {i}"));
            let nullifiers = self.money_call_nullifiers(call).await?;
            self.mark_spent_coins(None, &owncoins_nullifiers, &nullifiers, &None, &tx_hash)?;
        }

        Ok(())
    }

    /// Marks all coins in the wallet as spent, if their nullifier is in the given set.
    /// Returns a flag indicating if any of the provided nullifiers refer to our own wallet.
    pub fn mark_spent_coins(
        &self,
        mut tree: Option<&mut MerkleTree>,
        owncoins_nullifiers: &BTreeMap<[u8; 32], ([u8; 32], Position)>,
        nullifiers: &[Nullifier],
        spent_height: &Option<u32>,
        spent_tx_hash: &String,
    ) -> Result<bool> {
        if nullifiers.is_empty() {
            return Ok(false)
        }

        // Find our owncoins that where spent
        let mut spent_owncoins = Vec::new();
        for nullifier in nullifiers {
            if let Some(coin) = owncoins_nullifiers.get(&nullifier.to_bytes()) {
                spent_owncoins.push(coin);
            }
        }
        if spent_owncoins.is_empty() {
            return Ok(false)
        }

        // Create an SQL `UPDATE` query to mark rows as spent(1)
        let query = format!(
            "UPDATE {} SET {} = 1, {} = ?1, {} = ?2 WHERE {} = ?3;",
            *MONEY_COINS_TABLE,
            MONEY_COINS_COL_IS_SPENT,
            MONEY_COINS_COL_SPENT_HEIGHT,
            MONEY_COINS_COL_SPENT_TX_HASH,
            MONEY_COINS_COL_COIN
        );

        // Mark spent own coins
        for (ownoin, leaf_position) in spent_owncoins {
            // Execute the query
            if let Err(e) =
                self.wallet.exec_sql(&query, rusqlite::params![spent_height, spent_tx_hash, ownoin])
            {
                return Err(Error::DatabaseError(format!(
                    "[mark_spent_coins] Marking spent coin failed: {e}"
                )))
            }

            // Remove the coin mark from the Merkle tree
            if let Some(ref mut tree) = tree {
                tree.remove_mark(*leaf_position);
            }
        }

        Ok(true)
    }

    /// Inserts given slice to the wallets nullifiers Sparse Merkle Tree.
    pub fn smt_insert(&self, smt: &mut CacheSmt, nullifiers: &[Nullifier]) -> Result<()> {
        let leaves: Vec<_> = nullifiers.iter().map(|x| (x.inner(), x.inner())).collect();
        Ok(smt.insert_batch(leaves)?)
    }

    /// Reset the Money Merkle tree in the cache.
    pub fn reset_money_tree(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting Money Merkle tree"));
        if let Err(e) = self.cache.merkle_trees.remove(SLED_MERKLE_TREES_MONEY) {
            output.push(format!("[reset_money_tree] Resetting Money Merkle tree failed: {e}"));
            return Err(WalletDbError::GenericError)
        }
        output.push(String::from("Successfully reset Money Merkle tree"));

        Ok(())
    }

    /// Reset the Money nullifiers Sparse Merkle Tree in the cache.
    pub fn reset_money_smt(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting Money Sparse Merkle tree"));
        if let Err(e) = self.cache.money_smt.clear() {
            output
                .push(format!("[reset_money_smt] Resetting Money Sparse Merkle tree failed: {e}"));
            return Err(WalletDbError::GenericError)
        }
        output.push(String::from("Successfully reset Money Sparse Merkle tree"));

        Ok(())
    }

    /// Reset the Money coins in the wallet.
    pub fn reset_money_coins(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting coins"));
        let query = format!("DELETE FROM {};", *MONEY_COINS_TABLE);
        self.wallet.exec_sql(&query, &[])?;
        output.push(String::from("Successfully reset coins"));

        Ok(())
    }

    /// Remove the Money coins in the wallet that were created after
    /// provided height.
    pub fn remove_money_coins_after(
        &self,
        height: &u32,
        output: &mut Vec<String>,
    ) -> WalletDbResult<()> {
        output.push(format!("Removing coins after: {height}"));
        let query = format!(
            "DELETE FROM {} WHERE {} > ?1;",
            *MONEY_COINS_TABLE, MONEY_COINS_COL_CREATION_HEIGHT
        );
        self.wallet.exec_sql(&query, rusqlite::params![height])?;
        output.push(String::from("Successfully removed coins"));

        Ok(())
    }

    /// Mark the Money coins in the wallet that were spent after
    /// provided height as unspent.
    pub fn unspent_money_coins_after(
        &self,
        height: &u32,
        output: &mut Vec<String>,
    ) -> WalletDbResult<()> {
        output.push(format!("Unspenting coins after: {height}"));
        let query = format!(
            "UPDATE {} SET {} = 0, {} = NULL, {} = '=' WHERE {} > ?1;",
            *MONEY_COINS_TABLE,
            MONEY_COINS_COL_IS_SPENT,
            MONEY_COINS_COL_SPENT_HEIGHT,
            MONEY_COINS_COL_SPENT_TX_HASH,
            MONEY_COINS_COL_SPENT_HEIGHT
        );
        self.wallet.exec_sql(&query, rusqlite::params![Some(*height)])?;
        output.push(String::from("Successfully unspent coins"));

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

    /// Create and append a `Money::Fee` call to a given [`Transaction`].
    ///
    /// Optionally takes a set of spent coins in order not to reuse them here.
    ///
    /// Returns the `Fee` call, and all necessary data and parameters related.
    pub async fn append_fee_call(
        &self,
        tx: &Transaction,
        money_merkle_tree: &MerkleTree,
        fee_pk: &ProvingKey,
        fee_zkbin: &ZkBinary,
        spent_coins: Option<&[OwnCoin]>,
    ) -> Result<(ContractCall, Vec<Proof>, Vec<SecretKey>)> {
        // First we verify the fee-less transaction to see how much fee it requires for execution
        // and verification.
        let required_fee = compute_fee(&FEE_CALL_GAS) + self.get_tx_fee(tx, false).await?;

        // Knowing the total gas, we can now find an OwnCoin of enough value
        // so that we can create a valid Money::Fee call.
        let mut available_coins = self.get_token_coins(&DARK_TOKEN_ID).await?;
        available_coins.retain(|x| x.note.value > required_fee);
        if let Some(spent_coins) = spent_coins {
            available_coins.retain(|x| !spent_coins.contains(x));
        }
        if available_coins.is_empty() {
            return Err(Error::Custom("Not enough native tokens to pay for fees".to_string()))
        }

        let coin = &available_coins[0];
        let change_value = coin.note.value - required_fee;

        // Input and output setup
        let input = FeeCallInput {
            coin: coin.clone(),
            merkle_path: money_merkle_tree.witness(coin.leaf_position, 0).unwrap(),
            user_data_blind: BaseBlind::random(&mut OsRng),
        };

        let output = FeeCallOutput {
            public_key: PublicKey::from_secret(coin.secret),
            value: change_value,
            token_id: coin.note.token_id,
            blind: BaseBlind::random(&mut OsRng),
            spend_hook: FuncId::none(),
            user_data: pallas::Base::ZERO,
        };

        // Create blinding factors
        let token_blind = BaseBlind::random(&mut OsRng);
        let input_value_blind = ScalarBlind::random(&mut OsRng);
        let fee_value_blind = ScalarBlind::random(&mut OsRng);
        let output_value_blind = compute_remainder_blind(&[input_value_blind], &[fee_value_blind]);

        // Create an ephemeral signing key
        let signature_secret = SecretKey::random(&mut OsRng);

        // Create the actual fee proof
        let (proof, public_inputs) = create_fee_proof(
            fee_zkbin,
            fee_pk,
            &input,
            input_value_blind,
            &output,
            output_value_blind,
            output.spend_hook,
            output.user_data,
            output.blind,
            token_blind,
            signature_secret,
        )?;

        // Encrypted note for the output
        let note = MoneyNote {
            coin_blind: output.blind,
            value: output.value,
            token_id: output.token_id,
            spend_hook: output.spend_hook,
            user_data: output.user_data,
            value_blind: output_value_blind,
            token_blind,
            memo: vec![],
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;

        let params = MoneyFeeParamsV1 {
            input: Input {
                value_commit: public_inputs.input_value_commit,
                token_commit: public_inputs.token_commit,
                nullifier: public_inputs.nullifier,
                merkle_root: public_inputs.merkle_root,
                user_data_enc: public_inputs.input_user_data_enc,
                signature_public: public_inputs.signature_public,
            },
            output: Output {
                value_commit: public_inputs.output_value_commit,
                token_commit: public_inputs.token_commit,
                coin: public_inputs.output_coin,
                note: encrypted_note,
            },
            fee_value_blind,
            token_blind,
        };

        // Encode the contract call
        let mut data = vec![MoneyFunction::FeeV1 as u8];
        required_fee.encode_async(&mut data).await?;
        params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        Ok((call, vec![proof], vec![signature_secret]))
    }

    /// Create and attach the fee call to given transaction.
    pub async fn attach_fee(&self, tx: &mut Transaction) -> Result<()> {
        // Grab spent coins nullifiers of the transactions and check no other fee call exists
        let mut tx_nullifiers = vec![];
        for call in &tx.calls {
            if call.data.contract_id != *MONEY_CONTRACT_ID {
                continue
            }

            match MoneyFunction::try_from(call.data.data[0])? {
                MoneyFunction::FeeV1 => {
                    return Err(Error::Custom("Fee call already exists".to_string()))
                }
                _ => { /* Do nothing */ }
            }

            let nullifiers = self.money_call_nullifiers(call).await?;
            tx_nullifiers.extend_from_slice(&nullifiers);
        }

        // Grab all native owncoins to check if any is spent
        let mut spent_coins = vec![];
        let available_coins = self.get_token_coins(&DARK_TOKEN_ID).await?;
        for coin in available_coins {
            if tx_nullifiers.contains(&coin.nullifier()) {
                spent_coins.push(coin);
            }
        }

        // Now we need to do a lookup for the zkas proof bincodes, and create
        // the circuit objects and proving keys so we can build the transaction.
        // We also do this through the RPC.
        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;

        let Some(fee_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_FEE_NS_V1)
        else {
            return Err(Error::Custom("Fee circuit not found".to_string()))
        };

        let fee_zkbin = ZkBinary::decode(&fee_zkbin.1)?;

        let fee_circuit = ZkCircuit::new(empty_witnesses(&fee_zkbin)?, &fee_zkbin);

        // Creating Fee circuits proving keys
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit);

        // We first have to execute the fee-less tx to gather its used gas, and then we feed
        // it into the fee-creating function.
        let tree = self.get_money_tree().await?;
        let (fee_call, fee_proofs, fee_secrets) =
            self.append_fee_call(tx, &tree, &fee_pk, &fee_zkbin, Some(&spent_coins)).await?;

        // Append the fee call to the transaction
        tx.calls.push(DarkLeaf { data: fee_call, parent_index: None, children_indexes: vec![] });
        tx.proofs.push(fee_proofs);
        let sigs = tx.create_sigs(&fee_secrets)?;
        tx.signatures.push(sigs);

        Ok(())
    }
}
