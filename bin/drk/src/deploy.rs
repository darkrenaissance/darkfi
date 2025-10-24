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
}

// DEPLOY_AUTH_TABLE
pub const DEPLOY_AUTH_COL_ID: &str = "id";
pub const DEPLOY_AUTH_COL_DEPLOY_AUTHORITY: &str = "deploy_authority";
pub const DEPLOY_AUTH_COL_IS_FROZEN: &str = "is_frozen";
pub const DEPLOY_AUTH_COL_FREEZE_HEIGHT: &str = "freeze_height";

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

        let keypair = Keypair::random(&mut OsRng);
        let freeze_height: Option<u32> = None;

        let query = format!(
            "INSERT INTO {} ({}, {}, {}) VALUES (?1, ?2, ?3);",
            *DEPLOY_AUTH_TABLE,
            DEPLOY_AUTH_COL_DEPLOY_AUTHORITY,
            DEPLOY_AUTH_COL_IS_FROZEN,
            DEPLOY_AUTH_COL_FREEZE_HEIGHT,
        );
        self.wallet.exec_sql(
            &query,
            rusqlite::params![serialize_async(&keypair).await, 0, freeze_height],
        )?;

        output.push(String::from("Created new contract deploy authority"));
        output.push(format!("Contract ID: {}", ContractId::derive_public(keypair.public)));

        Ok(())
    }

    /// Reset all token deploy authorities frozen status in the wallet.
    pub fn reset_deploy_authorities(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting deploy authorities frozen status"));
        let query = format!(
            "UPDATE {} SET {} = 0, {} = NULL;",
            *DEPLOY_AUTH_TABLE, DEPLOY_AUTH_COL_IS_FROZEN, DEPLOY_AUTH_COL_FREEZE_HEIGHT
        );
        self.wallet.exec_sql(&query, &[])?;
        output.push(String::from("Successfully reset deploy authorities frozen status"));

        Ok(())
    }

    /// Remove deploy authorities frozen status in the wallet that
    /// where frozen after provided height.
    pub fn unfreeze_deploy_authorities_after(
        &self,
        height: &u32,
        output: &mut Vec<String>,
    ) -> WalletDbResult<()> {
        output.push(format!("Resetting deploy authorities frozen status after: {height}"));
        let query = format!(
            "UPDATE {} SET {} = 0, {} = NULL WHERE {} > ?1;",
            *DEPLOY_AUTH_TABLE,
            DEPLOY_AUTH_COL_IS_FROZEN,
            DEPLOY_AUTH_COL_FREEZE_HEIGHT,
            DEPLOY_AUTH_COL_FREEZE_HEIGHT
        );
        self.wallet.exec_sql(&query, rusqlite::params![Some(*height)])?;
        output.push(String::from("Successfully reset deploy authorities frozen status"));

        Ok(())
    }

    /// List contract deploy authorities from the wallet
    pub async fn list_deploy_auth(&self) -> Result<Vec<(i64, ContractId, bool, Option<u32>)>> {
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
            let Value::Integer(idx) = row[0] else {
                return Err(Error::ParseFailed("[list_deploy_auth] Failed to parse index"))
            };

            let Value::Blob(ref auth_bytes) = row[1] else {
                return Err(Error::ParseFailed("[list_deploy_auth] Failed to parse keypair bytes"))
            };
            let deploy_auth: Keypair = deserialize_async(auth_bytes).await?;

            let Value::Integer(frozen) = row[2] else {
                return Err(Error::ParseFailed("[list_deploy_auth] Failed to parse \"is_frozen\""))
            };

            let freeze_height = match row[3] {
                Value::Integer(freeze_height) => {
                    let Ok(freeze_height) = u32::try_from(freeze_height) else {
                        return Err(Error::ParseFailed(
                            "[list_deploy_auth] Freeze height parsing failed",
                        ))
                    };
                    Some(freeze_height)
                }
                Value::Null => None,
                _ => {
                    return Err(Error::ParseFailed(
                        "[list_deploy_auth] Freeze height parsing failed",
                    ))
                }
            };

            ret.push((
                idx,
                ContractId::derive_public(deploy_auth.public),
                frozen != 0,
                freeze_height,
            ))
        }

        Ok(ret)
    }

    /// Retrieve a deploy authority keypair and status for provided
    /// index.
    async fn get_deploy_auth(&self, idx: u64) -> Result<(Keypair, bool)> {
        // Find the deploy authority keypair
        let row = match self.wallet.query_single(
            &DEPLOY_AUTH_TABLE,
            &[DEPLOY_AUTH_COL_DEPLOY_AUTHORITY, DEPLOY_AUTH_COL_IS_FROZEN],
            convert_named_params! {(DEPLOY_AUTH_COL_ID, idx)},
        ) {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[get_deploy_auth] Failed to retrieve deploy authority keypair: {e}"
                )))
            }
        };

        let Value::Blob(ref keypair_bytes) = row[0] else {
            return Err(Error::ParseFailed("[get_deploy_auth] Failed to parse keypair bytes"))
        };
        let keypair: Keypair = deserialize_async(keypair_bytes).await?;

        let Value::Integer(locked) = row[1] else {
            return Err(Error::ParseFailed("[get_deploy_auth] Failed to parse \"is_frozen\""))
        };

        Ok((keypair, locked != 0))
    }

    /// Retrieve contract deploy authorities public keys from the
    /// wallet.
    pub async fn get_deploy_auths_keys_map(&self) -> Result<HashMap<[u8; 32], SecretKey>> {
        let rows = match self.wallet.query_multiple(
            &DEPLOY_AUTH_TABLE,
            &[DEPLOY_AUTH_COL_DEPLOY_AUTHORITY],
            &[],
        ) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                "[get_deploy_auths_keys_map] Failed to retrieve deploy authorities keypairs: {e}",
            )))
            }
        };

        let mut ret = HashMap::new();
        for row in rows {
            let Value::Blob(ref keypair_bytes) = row[0] else {
                return Err(Error::ParseFailed(
                    "[get_deploy_auths_keys_map] Failed to parse keypair bytes",
                ))
            };
            let keypair: Keypair = deserialize_async(keypair_bytes).await?;
            ret.insert(keypair.public.to_bytes(), keypair.secret);
        }

        Ok(ret)
    }

    /// Auxiliary function to apply `DeployFunction::DeployV1` call
    /// data to the wallet.
    /// Returns a flag indicating if the provided call refers to our
    /// own wallet.
    fn apply_deploy_deploy_data(
        &self,
        scan_cache: &ScanCache,
        params: &DeployParamsV1,
        _tx_hash: &TransactionHash,
        _block_height: &u32,
    ) -> Result<bool> {
        // Check if we have the deploy authority key
        let Some(_secret_key) = scan_cache.own_deploy_auths.get(&params.public_key.to_bytes())
        else {
            return Ok(false)
        };

        // Create a new history record containing the deployment data
        // TODO

        Ok(true)
    }

    /// Auxiliary function to apply `DeployFunction::LockV1` call
    /// data to the wallet.
    /// Returns a flag indicating if the provided call refers to our
    /// own wallet.
    fn apply_deploy_lock_data(
        &self,
        scan_cache: &ScanCache,
        public_key: &PublicKey,
        _tx_hash: &TransactionHash,
        freeze_height: &u32,
    ) -> Result<bool> {
        // Check if we have the deploy authority key
        let Some(secret_key) = scan_cache.own_deploy_auths.get(&public_key.to_bytes()) else {
            return Ok(false)
        };

        // Freeze contract
        let query = format!(
            "UPDATE {} SET {} = 1, {} = ?1 WHERE {} = ?2;",
            *DEPLOY_AUTH_TABLE,
            DEPLOY_AUTH_COL_IS_FROZEN,
            DEPLOY_AUTH_COL_FREEZE_HEIGHT,
            DEPLOY_AUTH_COL_DEPLOY_AUTHORITY
        );
        if let Err(e) = self.wallet.exec_sql(
            &query,
            rusqlite::params![Some(*freeze_height), serialize(&Keypair::new(*secret_key))],
        ) {
            return Err(Error::DatabaseError(format!(
                "[apply_deploy_lock_data] Lock deploy authority failed: {e}"
            )))
        }

        // Create a new history record for the lock transaction
        // TODO

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
            }
        }
    }

    /// Create a feeless contract deployment transaction.
    pub async fn deploy_contract(
        &self,
        deploy_auth: u64,
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

        // Create the TransactionBuilder containing above cal
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
    pub async fn lock_contract(&self, deploy_auth: u64) -> Result<Transaction> {
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

        // Create the TransactionBuilder containing above cal
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
