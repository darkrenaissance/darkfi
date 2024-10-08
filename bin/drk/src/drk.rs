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

use std::{fs, process::exit, sync::Arc};

use url::Url;

use darkfi::{rpc::client::RpcClient, util::path::expand_path, Result};

use crate::walletdb::{WalletDb, WalletPtr};

/// CLI-util structure
pub struct Drk {
    /// Wallet database operations handler
    pub wallet: WalletPtr,
    /// JSON-RPC client to execute requests to darkfid daemon
    pub rpc_client: Option<RpcClient>,
    /// Flag indicating if fun stuff are enabled
    pub fun: bool,
}

impl Drk {
    pub async fn new(
        wallet_path: String,
        wallet_pass: String,
        endpoint: Option<Url>,
        ex: Arc<smol::Executor<'static>>,
        fun: bool,
    ) -> Result<Self> {
        // Script kiddies protection
        if wallet_pass == "changeme" {
            eprintln!("Please don't use default wallet password...");
            exit(2);
        }

        // Initialize wallet
        let wallet_path = expand_path(&wallet_path)?;
        if !wallet_path.exists() {
            if let Some(parent) = wallet_path.parent() {
                fs::create_dir_all(parent)?;
            }
        }
        let wallet = match WalletDb::new(Some(wallet_path), Some(&wallet_pass)) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Error initializing wallet: {e:?}");
                exit(2);
            }
        };

        // Initialize rpc client
        let rpc_client = if let Some(endpoint) = endpoint {
            Some(RpcClient::new(endpoint, ex).await?)
        } else {
            None
        };

        Ok(Self { wallet, rpc_client, fun })
    }

    /// Initialize wallet with tables for drk
    pub fn initialize_wallet(&self) -> Result<()> {
        let wallet_schema = include_str!("../wallet.sql");
        if let Err(e) = self.wallet.exec_batch_sql(wallet_schema) {
            eprintln!("Error initializing wallet: {e:?}");
            exit(2);
        }

        Ok(())
    }
}
