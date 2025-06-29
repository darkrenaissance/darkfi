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

use lazy_static::lazy_static;
use rand::rngs::OsRng;

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Error, Result,
};
use darkfi_deployooor_contract::{
    client::{deploy_v1::DeployCallBuilder, lock_v1::LockCallBuilder},
    DeployFunction,
};
use darkfi_sdk::{
    crypto::{ContractId, Keypair, DEPLOYOOOR_CONTRACT_ID},
    ContractCall,
};
use darkfi_serial::{deserialize_async, serialize_async, AsyncEncodable};
use rusqlite::types::Value;

use crate::{convert_named_params, error::WalletDbResult, Drk};

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

    /// Retrieve a deploy authority keypair given an index
    async fn get_deploy_auth(&self, idx: u64) -> Result<Keypair> {
        // Find the deploy authority keypair
        let row = match self.wallet.query_single(
            &DEPLOY_AUTH_TABLE,
            &[DEPLOY_AUTH_COL_DEPLOY_AUTHORITY],
            convert_named_params! {(DEPLOY_AUTH_COL_ID, idx)},
        ) {
            Ok(v) => v,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[deploy_contract] Failed to retrieve deploy authority keypair: {e}"
                )))
            }
        };

        let Value::Blob(ref keypair_bytes) = row[0] else {
            return Err(Error::ParseFailed("[deploy_contract] Failed to parse keypair bytes"))
        };
        let keypair: Keypair = deserialize_async(keypair_bytes).await?;

        Ok(keypair)
    }

    /// Create a feeless contract deployment transaction.
    pub async fn deploy_contract(
        &self,
        deploy_auth: u64,
        wasm_bincode: Vec<u8>,
        deploy_ix: Vec<u8>,
    ) -> Result<Transaction> {
        // Fetch the keypair
        let deploy_keypair = self.get_deploy_auth(deploy_auth).await?;

        // Create the contract call
        let deploy_call = DeployCallBuilder { deploy_keypair, wasm_bincode, deploy_ix };
        let deploy_debris = deploy_call.build()?;

        // Encode the call
        let mut data = vec![DeployFunction::DeployV1 as u8];
        deploy_debris.params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *DEPLOYOOOR_CONTRACT_ID, data };
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: vec![] }, vec![])?;

        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[deploy_keypair.secret])?;
        tx.signatures = vec![sigs];

        Ok(tx)
    }

    /// Create a feeless contract redeployment lock transaction.
    pub async fn lock_contract(&self, deploy_auth: u64) -> Result<Transaction> {
        // Fetch the keypair
        let deploy_keypair = self.get_deploy_auth(deploy_auth).await?;

        // Create the contract call
        let lock_call = LockCallBuilder { deploy_keypair };
        let lock_debris = lock_call.build()?;

        // Encode the call
        let mut data = vec![DeployFunction::LockV1 as u8];
        lock_debris.params.encode_async(&mut data).await?;
        let call = ContractCall { contract_id: *DEPLOYOOOR_CONTRACT_ID, data };
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: vec![] }, vec![])?;

        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[deploy_keypair.secret])?;
        tx.signatures = vec![sigs];

        Ok(tx)
    }
}
