/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use lazy_static::lazy_static;
use rand::rngs::OsRng;

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_deployooor_contract::{
    client::{deploy_v1::DeployCallBuilder, lock_v1::LockCallBuilder},
    model::LockParamsV1,
    DeployFunction,
};
use darkfi_money_contract::MONEY_CONTRACT_ZKAS_FEE_NS_V1;
use darkfi_sdk::{
    crypto::{
        ContractId, Keypair, PublicKey, SecretKey, DEPLOYOOOR_CONTRACT_ID, MONEY_CONTRACT_ID,
    },
    deploy::DeployParamsV1,
    tx::TransactionHash,
    ContractCall,
};
use darkfi_serial::{deserialize_async, serialize, serialize_async, AsyncEncodable};
use rusqlite::types::Value;

use crate::{convert_named_params, error::WalletDbResult, rpc::ScanCache, Drk};

// Wallet SQL table constant names. These have to represent the `wallet.sql`
// SQL schema. Table names are prefixed with the contract ID to avoid collisions.
lazy_static! {
    pub static ref DEPLOY_AUTH_TABLE: String =
        format!("{}_deploy_auth", DEPLOYOOOR_CONTRACT_ID.to_string());
    pub static ref DEPLOY_HISTORY_TABLE: String =
        format!("{}_deploy_history", DEPLOYOOOR_CONTRACT_ID.to_string());
}

// DEPLOY_AUTH_TABLE
pub const DEPLOY_AUTH_COL_CONTRACT_ID: &str = "contract_id";
pub const DEPLOY_AUTH_COL_SECRET_KEY: &str = "secret_key";
pub const DEPLOY_AUTH_COL_IS_LOCKED: &str = "is_locked";
pub const DEPLOY_AUTH_COL_LOCK_HEIGHT: &str = "lock_height";

// DEPLOY_HISTORY_TABLE
pub const DEPLOY_HISTORY_COL_TX_HASH: &str = "tx_hash";
pub const DEPLOY_HISTORY_COL_CONTRACT: &str = "contract";
pub const DEPLOY_HISTORY_COL_TYPE: &str = "type";
pub const DEPLOY_HISTORY_COL_BLOCK_HEIGHT: &str = "block_height";
pub const DEPLOY_HISTORY_COL_WASM_BINCODE: &str = "wasm_bincode";
pub const DEPLOY_HISTORY_COL_DEPLOY_IX: &str = "deploy_ix";

impl Drk {
    /// Initialize wallet with tables for the Deployooor contract.
    pub fn initialize_deployooor(&self) -> WalletDbResult<()> {
        // Initialize Deployooor wallet schema
        let wallet_schema = include_str!("../deploy.sql");
        self.wallet.exec_batch_sql(wallet_schema)?;

        Ok(())
    }

    /// Generate a new deploy authority keypair and place it into the wallet
    pub async fn deploy_auth_keygen(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Generating a new keypair"));

        let secret_key = SecretKey::random(&mut OsRng);
        let contract_id = ContractId::derive_public(PublicKey::from_secret(secret_key));
        let lock_height: Option<u32> = None;

        let query = format!(
            "INSERT INTO {} ({}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4);",
            *DEPLOY_AUTH_TABLE,
            DEPLOY_AUTH_COL_CONTRACT_ID,
            DEPLOY_AUTH_COL_SECRET_KEY,
            DEPLOY_AUTH_COL_IS_LOCKED,
            DEPLOY_AUTH_COL_LOCK_HEIGHT,
        );
        self.wallet.exec_sql(
            &query,
            rusqlite::params![
                serialize_async(&contract_id).await,
                serialize_async(&secret_key).await,
                0,
                lock_height
            ],
        )?;

        output.push(String::from("Created new contract deploy authority"));
        output.push(format!("Contract ID: {contract_id}"));

        Ok(())
    }

    /// Insert a deploy authority history record into the wallet.
    pub fn put_deploy_history_record(
        &self,
        tx_hash: &TransactionHash,
        contract: &ContractId,
        tx_type: &str,
        block_height: &u32,
        wasm_bincode: &Option<Vec<u8>>,
        deploy_ix: &Option<Vec<u8>>,
    ) -> WalletDbResult<()> {
        let query = format!(
            "INSERT INTO {} ({}, {}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5, ?6);",
            *DEPLOY_HISTORY_TABLE,
            DEPLOY_HISTORY_COL_TX_HASH,
            DEPLOY_HISTORY_COL_CONTRACT,
            DEPLOY_HISTORY_COL_TYPE,
            DEPLOY_HISTORY_COL_BLOCK_HEIGHT,
            DEPLOY_HISTORY_COL_WASM_BINCODE,
            DEPLOY_HISTORY_COL_DEPLOY_IX,
        );
        self.wallet.exec_sql(
            &query,
            rusqlite::params![
                tx_hash.to_string(),
                serialize(contract),
                tx_type,
                block_height,
                serialize(wasm_bincode),
                serialize(deploy_ix),
            ],
        )?;

        Ok(())
    }

    /// Reset all contract deploy authorities locked status in the wallet.
    pub fn reset_deploy_authorities(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting deploy authorities locked status"));
        let query = format!(
            "UPDATE {} SET {} = 0, {} = NULL;",
            *DEPLOY_AUTH_TABLE, DEPLOY_AUTH_COL_IS_LOCKED, DEPLOY_AUTH_COL_LOCK_HEIGHT
        );
        self.wallet.exec_sql(&query, &[])?;
        output.push(String::from("Successfully reset deploy authorities locked status"));

        Ok(())
    }

    /// Remove deploy authorities locked status in the wallet that
    /// where locked after provided height.
    pub fn unlock_deploy_authorities_after(
        &self,
        height: &u32,
        output: &mut Vec<String>,
    ) -> WalletDbResult<()> {
        output.push(format!("Resetting deploy authorities locked status after: {height}"));
        let query = format!(
            "UPDATE {} SET {} = 0, {} = NULL WHERE {} > ?1;",
            *DEPLOY_AUTH_TABLE,
            DEPLOY_AUTH_COL_IS_LOCKED,
            DEPLOY_AUTH_COL_LOCK_HEIGHT,
            DEPLOY_AUTH_COL_LOCK_HEIGHT
        );
        self.wallet.exec_sql(&query, rusqlite::params![Some(*height)])?;
        output.push(String::from("Successfully reset deploy authorities locked status"));

        Ok(())
    }

    /// Reset all contracts history records in the wallet.
    pub fn reset_deploy_history(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting deployment history"));
        let query = format!("DELETE FROM {};", *DEPLOY_HISTORY_TABLE);
        self.wallet.exec_sql(&query, &[])?;
        output.push(String::from("Successfully deployment history"));

        Ok(())
    }

    /// Remove the contracts history records in the wallet that were
    /// created after provided height.
    pub fn remove_deploy_history_after(
        &self,
        height: &u32,
        output: &mut Vec<String>,
    ) -> WalletDbResult<()> {
        output.push(format!("Removing deployment history records after: {height}"));
        let query = format!(
            "DELETE FROM {} WHERE {} > ?1;",
            *DEPLOY_HISTORY_TABLE, DEPLOY_HISTORY_COL_BLOCK_HEIGHT
        );
        self.wallet.exec_sql(&query, rusqlite::params![height])?;
        output.push(String::from("Successfully removed deployment history records"));

        Ok(())
    }

    /// List contract deploy authorities from the wallet
    pub async fn list_deploy_auth(
        &self,
    ) -> Result<Vec<(ContractId, SecretKey, bool, Option<u32>)>> {
        let rows = match self.wallet.query_multiple(&DEPLOY_AUTH_TABLE, &[], &[]) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[list_deploy_auth] Deploy auth retrieval failed: {e}",
                )))
            }
        };

        let mut ret = Vec::with_capacity(rows.len());
        for row in rows {
            let Value::Blob(ref contract_id_bytes) = row[0] else {
                return Err(Error::ParseFailed(
                    "[list_deploy_auth] Failed to parse contract id bytes",
                ))
            };
            let contract_id: ContractId = deserialize_async(contract_id_bytes).await?;

            let Value::Blob(ref secret_key_bytes) = row[1] else {
                return Err(Error::ParseFailed(
                    "[list_deploy_auth] Failed to parse secret key bytes",
                ))
            };
            let secret_key: SecretKey = deserialize_async(secret_key_bytes).await?;

            let Value::Integer(locked) = row[2] else {
                return Err(Error::ParseFailed("[list_deploy_auth] Failed to parse \"is_locked\""))
            };

            let lock_height = match row[3] {
                Value::Integer(lock_height) => {
                    let Ok(lock_height) = u32::try_from(lock_height) else {
                        return Err(Error::ParseFailed(
                            "[list_deploy_auth] Lock height parsing failed",
                        ))
                    };
                    Some(lock_height)
                }
                Value::Null => None,
                _ => {
                    return Err(Error::ParseFailed("[list_deploy_auth] Lock height parsing failed"))
                }
            };

            ret.push((contract_id, secret_key, locked != 0, lock_height))
        }

        Ok(ret)
    }

    /// Retrieve a deploy authority keypair and status for provided
    /// contract id.
    async fn get_deploy_auth(&self, contract_id: &ContractId) -> Result<(Keypair, bool)> {
        // Find the deploy authority keypair
        let row = match self.wallet.query_single(
            &DEPLOY_AUTH_TABLE,
            &[DEPLOY_AUTH_COL_SECRET_KEY, DEPLOY_AUTH_COL_IS_LOCKED],
            convert_named_params! {(DEPLOY_AUTH_COL_CONTRACT_ID, serialize_async(contract_id).await)},
        ) {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[get_deploy_auth] Failed to retrieve deploy authority keypair: {e}"
                )))
            }
        };

        let Value::Blob(ref secret_key_bytes) = row[0] else {
            return Err(Error::ParseFailed("[get_deploy_auth] Failed to parse secret key bytes"))
        };
        let secret_key: SecretKey = deserialize_async(secret_key_bytes).await?;
        let keypair = Keypair::new(secret_key);

        let Value::Integer(locked) = row[1] else {
            return Err(Error::ParseFailed("[get_deploy_auth] Failed to parse \"is_locked\""))
        };

        Ok((keypair, locked != 0))
    }

    /// Retrieve contract deploy authorities keys map from the wallet.
    pub async fn get_deploy_auths_keys_map(&self) -> Result<HashMap<[u8; 32], SecretKey>> {
        let rows = match self.wallet.query_multiple(
            &DEPLOY_AUTH_TABLE,
            &[DEPLOY_AUTH_COL_SECRET_KEY],
            &[],
        ) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                "[get_deploy_auths_keys_map] Failed to retrieve deploy authorities secret keys: {e}",
            )))
            }
        };

        let mut ret = HashMap::new();
        for row in rows {
            let Value::Blob(ref secret_key_bytes) = row[0] else {
                return Err(Error::ParseFailed(
                    "[get_deploy_auths_keys_map] Failed to parse secret key bytes",
                ))
            };
            let secret_key: SecretKey = deserialize_async(secret_key_bytes).await?;
            ret.insert(PublicKey::from_secret(secret_key).to_bytes(), secret_key);
        }

        Ok(ret)
    }

    /// Retrieve all deploy history records basic information, for
    /// provided contract id.
    pub async fn get_deploy_auth_history(
        &self,
        contract_id: &ContractId,
    ) -> Result<Vec<(String, String, u32)>> {
        let rows = match self.wallet.query_multiple(
            &DEPLOY_HISTORY_TABLE,
            &[DEPLOY_HISTORY_COL_TX_HASH, DEPLOY_HISTORY_COL_TYPE, DEPLOY_HISTORY_COL_BLOCK_HEIGHT],
            convert_named_params! {(DEPLOY_HISTORY_COL_CONTRACT, serialize_async(contract_id).await)},
        ) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                "[get_deploy_auth_history] Failed to retrieve deploy authority history records: {e}",
            )))
            }
        };

        let mut ret = Vec::with_capacity(rows.len());
        for row in rows {
            let Value::Text(ref tx_hash) = row[0] else {
                return Err(Error::ParseFailed(
                    "[get_deploy_auth_history] Transaction hash parsing failed",
                ))
            };

            let Value::Text(ref tx_type) = row[1] else {
                return Err(Error::ParseFailed("[get_deploy_auth_history] Type parsing failed"))
            };

            let Value::Integer(block_height) = row[2] else {
                return Err(Error::ParseFailed(
                    "[get_deploy_auth_history] Block height parsing failed",
                ))
            };
            let Ok(block_height) = u32::try_from(block_height) else {
                return Err(Error::ParseFailed(
                    "[get_deploy_auth_history] Block height parsing failed",
                ))
            };

            ret.push((tx_hash.clone(), tx_type.clone(), block_height));
        }

        Ok(ret)
    }

    /// Retrieve deploy history record WASM bincode and deployed
    /// instruction, for provided transaction hash.
    pub async fn get_deploy_history_record_data(
        &self,
        tx_hash: &str,
    ) -> Result<(Option<Vec<u8>>, Option<Vec<u8>>)> {
        let row = match self.wallet.query_single(
            &DEPLOY_HISTORY_TABLE,
            &[DEPLOY_HISTORY_COL_WASM_BINCODE, DEPLOY_HISTORY_COL_DEPLOY_IX],
            convert_named_params! {(DEPLOY_HISTORY_COL_TX_HASH, tx_hash)},
        ) {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[get_deploy_history_record] Failed to retrieve deploy history record: {e}"
                )))
            }
        };

        let Value::Blob(ref wasm_bincode_bytes) = row[0] else {
            return Err(Error::ParseFailed(
                "[get_deploy_history_record] Failed to parse wasm bincode bytes",
            ))
        };
        let wasm_bincode: Option<Vec<u8>> = deserialize_async(wasm_bincode_bytes).await?;

        let Value::Blob(ref deploy_ix_bytes) = row[1] else {
            return Err(Error::ParseFailed(
                "[get_deploy_history_record] Failed to parse deploy ix bytes",
            ))
        };
        let deploy_ix: Option<Vec<u8>> = deserialize_async(deploy_ix_bytes).await?;

        Ok((wasm_bincode, deploy_ix))
    }

    /// Auxiliary function to apply `DeployFunction::DeployV1` call
    /// data to the wallet.
    /// Returns a flag indicating if the provided call refers to our
    /// own wallet.
    fn apply_deploy_deploy_data(
        &self,
        scan_cache: &ScanCache,
        params: &DeployParamsV1,
        tx_hash: &TransactionHash,
        block_height: &u32,
    ) -> Result<bool> {
        // Check if we have the deploy authority key
        let Some(_) = scan_cache.own_deploy_auths.get(&params.public_key.to_bytes()) else {
            return Ok(false)
        };

        // Create a new history record containing the deployment data
        if let Err(e) = self.put_deploy_history_record(
            tx_hash,
            &ContractId::derive_public(params.public_key),
            "DEPLOYMENT",
            block_height,
            &Some(params.wasm_bincode.clone()),
            &Some(params.ix.clone()),
        ) {
            return Err(Error::DatabaseError(format!(
                "[apply_deploy_deploy_data] Inserting deploy history recod failed: {e}"
            )))
        }

        Ok(true)
    }

    /// Auxiliary function to apply `DeployFunction::LockV1` call
    /// data to the wallet.
    /// Returns a flag indicating if the provided call refers to our
    /// own wallet.
    async fn apply_deploy_lock_data(
        &self,
        scan_cache: &ScanCache,
        public_key: &PublicKey,
        tx_hash: &TransactionHash,
        lock_height: &u32,
    ) -> Result<bool> {
        // Check if we have the deploy authority key
        let Some(secret_key) = scan_cache.own_deploy_auths.get(&public_key.to_bytes()) else {
            return Ok(false)
        };

        // Lock contract
        let secret_key = serialize_async(secret_key).await;
        let query = format!(
            "UPDATE {} SET {} = 1, {} = ?1 WHERE {} = ?2;",
            *DEPLOY_AUTH_TABLE,
            DEPLOY_AUTH_COL_IS_LOCKED,
            DEPLOY_AUTH_COL_LOCK_HEIGHT,
            DEPLOY_AUTH_COL_SECRET_KEY
        );
        if let Err(e) =
            self.wallet.exec_sql(&query, rusqlite::params![Some(*lock_height), secret_key])
        {
            return Err(Error::DatabaseError(format!(
                "[apply_deploy_lock_data] Lock deploy authority failed: {e}"
            )))
        }

        // Create a new history record for the lock transaction
        if let Err(e) = self.put_deploy_history_record(
            tx_hash,
            &ContractId::derive_public(*public_key),
            "LOCK",
            lock_height,
            &None,
            &None,
        ) {
            return Err(Error::DatabaseError(format!(
                "[apply_deploy_lock_data] Inserting deploy history recod failed: {e}"
            )))
        }

        Ok(true)
    }

    /// Append data related to DeployoOor contract transactions into
    /// the wallet database and update the provided scan cache.
    /// Returns a flag indicating if provided data refer to our own
    /// wallet.
    pub async fn apply_tx_deploy_data(
        &self,
        scan_cache: &mut ScanCache,
        data: &[u8],
        tx_hash: &TransactionHash,
        block_height: &u32,
    ) -> Result<bool> {
        // Run through the transaction call data and see what we got:
        match DeployFunction::try_from(data[0])? {
            DeployFunction::DeployV1 => {
                scan_cache.log(String::from("[apply_tx_deploy_data] Found Deploy::DeployV1 call"));
                let params: DeployParamsV1 = deserialize_async(&data[1..]).await?;
                self.apply_deploy_deploy_data(scan_cache, &params, tx_hash, block_height)
            }
            DeployFunction::LockV1 => {
                scan_cache.log(String::from("[apply_tx_deploy_data] Found Deploy::LockV1 call"));
                let params: LockParamsV1 = deserialize_async(&data[1..]).await?;
                self.apply_deploy_lock_data(scan_cache, &params.public_key, tx_hash, block_height)
                    .await
            }
        }
    }

    /// Create a feeless contract deployment transaction.
    pub async fn deploy_contract(
        &self,
        deploy_auth: &ContractId,
        wasm_bincode: Vec<u8>,
        deploy_ix: Vec<u8>,
    ) -> Result<Transaction> {
        // Fetch the keypair and its status
        let (deploy_keypair, is_locked) = self.get_deploy_auth(deploy_auth).await?;

        // Check lock status
        if is_locked {
            return Err(Error::Custom("[deploy_contract] Contract is locked".to_string()))
        }

        // Now we need to do a lookup for the zkas proof bincodes, and create
        // the circuit objects and proving keys so we can build the transaction.
        // We also do this through the RPC.
        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;

        let Some(fee_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_FEE_NS_V1)
        else {
            return Err(Error::Custom("[deploy_contract] Fee circuit not found".to_string()))
        };

        let fee_zkbin = ZkBinary::decode(&fee_zkbin.1)?;

        let fee_circuit = ZkCircuit::new(empty_witnesses(&fee_zkbin)?, &fee_zkbin);

        // Creating Fee circuit proving keys
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit);

        // Create the contract call
        let deploy_call = DeployCallBuilder { deploy_keypair, wasm_bincode, deploy_ix };
        let deploy_debris = deploy_call.build()?;

        // Encode the call
        let mut data = vec![DeployFunction::DeployV1 as u8];
        deploy_debris.params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *DEPLOYOOOR_CONTRACT_ID, data };

        // Create the TransactionBuilder containing above call
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: vec![] }, vec![])?;

        // We first have to execute the fee-less tx to gather its used gas, and then we feed
        // it into the fee-creating function.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[deploy_keypair.secret])?;
        tx.signatures.push(sigs);

        let tree = self.get_money_tree().await?;
        let (fee_call, fee_proofs, fee_secrets) =
            self.append_fee_call(&tx, &tree, &fee_pk, &fee_zkbin, None).await?;

        // Append the fee call to the transaction
        tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;

        // Now build the actual transaction and sign it with all necessary keys.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[deploy_keypair.secret])?;
        tx.signatures.push(sigs);
        let sigs = tx.create_sigs(&fee_secrets)?;
        tx.signatures.push(sigs);

        Ok(tx)
    }

    /// Create a feeless contract redeployment lock transaction.
    pub async fn lock_contract(&self, deploy_auth: &ContractId) -> Result<Transaction> {
        // Fetch the keypair and its status
        let (deploy_keypair, is_locked) = self.get_deploy_auth(deploy_auth).await?;

        // Check lock status
        if is_locked {
            return Err(Error::Custom("[lock_contract] Contract is already locked".to_string()))
        }

        // Now we need to do a lookup for the zkas proof bincodes, and create
        // the circuit objects and proving keys so we can build the transaction.
        // We also do this through the RPC.
        let zkas_bins = self.lookup_zkas(&MONEY_CONTRACT_ID).await?;

        let Some(fee_zkbin) = zkas_bins.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_FEE_NS_V1)
        else {
            return Err(Error::Custom("[lock_contract] Fee circuit not found".to_string()))
        };

        let fee_zkbin = ZkBinary::decode(&fee_zkbin.1)?;

        let fee_circuit = ZkCircuit::new(empty_witnesses(&fee_zkbin)?, &fee_zkbin);

        // Creating Fee circuit proving keys
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit);

        // Create the contract call
        let lock_call = LockCallBuilder { deploy_keypair };
        let lock_debris = lock_call.build()?;

        // Encode the call
        let mut data = vec![DeployFunction::LockV1 as u8];
        lock_debris.params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *DEPLOYOOOR_CONTRACT_ID, data };

        // Create the TransactionBuilder containing above call
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: vec![] }, vec![])?;

        // We first have to execute the fee-less tx to gather its used gas, and then we feed
        // it into the fee-creating function.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[deploy_keypair.secret])?;
        tx.signatures.push(sigs);

        let tree = self.get_money_tree().await?;
        let (fee_call, fee_proofs, fee_secrets) =
            self.append_fee_call(&tx, &tree, &fee_pk, &fee_zkbin, None).await?;

        // Append the fee call to the transaction
        tx_builder.append(ContractCallLeaf { call: fee_call, proofs: fee_proofs }, vec![])?;

        // Now build the actual transaction and sign it with all necessary keys.
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[deploy_keypair.secret])?;
        tx.signatures.push(sigs);
        let sigs = tx.create_sigs(&fee_secrets)?;
        tx.signatures.push(sigs);

        Ok(tx)
    }
}
