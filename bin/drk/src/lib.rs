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

use std::{fs::create_dir_all, sync::Arc};

use smol::lock::RwLock;
use url::Url;

use darkfi::{system::ExecutorPtr, util::path::expand_path, Error, Result};
use darkfi_sdk::crypto::keypair::Network;

/// Error codes
pub mod error;
use error::{WalletDbError, WalletDbResult};

/// Common shared functions
pub mod common;

/// darkfid JSON-RPC related methods
pub mod rpc;
use rpc::DarkfidRpcClient;

/// Payment methods
pub mod transfer;

/// Swap methods
pub mod swap;

/// Token methods
pub mod token;

/// CLI utility functions
pub mod cli_util;

/// Drk interactive shell
pub mod interactive;

/// Wallet functionality related to Money
pub mod money;

/// Wallet functionality related to Dao
pub mod dao;

/// Wallet functionality related to Deployooor
pub mod deploy;

/// Wallet functionality related to transactions history
pub mod txs_history;

/// Wallet functionality related to scanned blocks
pub mod scanned_blocks;

/// Wallet database operations handler
pub mod walletdb;
use walletdb::{WalletDb, WalletPtr};

/// Blockchain cache database operations handler
pub mod cache;
use cache::Cache;

/// Atomic pointer to a `Drk` structure.
pub type DrkPtr = Arc<RwLock<Drk>>;

/// CLI-util structure
pub struct Drk {
    /// Blockchain network
    pub network: Network,
    /// Blockchain cache database operations handler
    pub cache: Cache,
    /// Wallet database operations handler
    pub wallet: WalletPtr,
    /// JSON-RPC client to execute requests to darkfid daemon
    pub rpc_client: Option<RwLock<DarkfidRpcClient>>,
    /// Flag indicating if fun stuff are enabled
    pub fun: bool,
}

impl Drk {
    pub async fn new(
        network: Network,
        cache_path: String,
        wallet_path: String,
        wallet_pass: String,
        endpoint: Option<Url>,
        ex: &ExecutorPtr,
        fun: bool,
    ) -> Result<Self> {
        // Initialize blockchain cache database
        let db_path = expand_path(&cache_path)?;
        let sled_db = sled_overlay::sled::open(&db_path)?;
        let Ok(cache) = Cache::new(&sled_db) else {
            return Err(Error::DatabaseError(format!("{}", WalletDbError::InitializationFailed)));
        };

        // Initialize wallet
        let wallet_path = expand_path(&wallet_path)?;
        if !wallet_path.exists() {
            if let Some(parent) = wallet_path.parent() {
                create_dir_all(parent)?;
            }
        }
        let Ok(wallet) = WalletDb::new(Some(wallet_path), Some(&wallet_pass)) else {
            return Err(Error::DatabaseError(format!("{}", WalletDbError::InitializationFailed)));
        };

        // Initialize rpc client
        let rpc_client = if let Some(endpoint) = endpoint {
            Some(RwLock::new(DarkfidRpcClient::new(endpoint, ex.clone()).await))
        } else {
            None
        };

        Ok(Self { network, cache, wallet, rpc_client, fun })
    }

    pub fn into_ptr(self) -> DrkPtr {
        Arc::new(RwLock::new(self))
    }

    /// Initialize wallet with tables for `Drk`.
    pub async fn initialize_wallet(&self) -> WalletDbResult<()> {
        // Initialize wallet schema
        self.wallet.exec_batch_sql(include_str!("../wallet.sql"))?;

        Ok(())
    }

    /// Auxiliary function to completely reset wallet state.
    pub fn reset(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting full wallet state"));
        self.reset_scanned_blocks(output)?;
        self.reset_money_tree(output)?;
        self.reset_money_smt(output)?;
        self.reset_money_coins(output)?;
        self.reset_mint_authorities(output)?;
        self.reset_dao_trees(output)?;
        self.reset_daos(output)?;
        self.reset_dao_proposals(output)?;
        self.reset_dao_votes(output)?;
        self.reset_deploy_authorities(output)?;
        self.reset_deploy_history(output)?;
        self.reset_tx_history(output)?;
        output.push(String::from("Successfully reset full wallet state"));
        Ok(())
    }
}
