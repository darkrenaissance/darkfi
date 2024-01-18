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

use std::{process::exit, sync::Arc, time::Instant};

use smol::stream::StreamExt;
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize, cli_desc,
    rpc::{client::RpcClient, jsonrpc::JsonRequest, util::JsonValue},
    util::path::expand_path,
    Result,
};

/// Error codes
mod error;

/// CLI utility functions
mod cli_util;
use cli_util::kaching;

/// Wallet database operations handler
mod walletdb;
use walletdb::{WalletDb, WalletPtr};

const CONFIG_FILE: &str = "drk_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../drk_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "drk", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long, default_value = "~/.local/darkfi/drk/wallet.db")]
    /// Path to wallet database
    wallet_path: String,

    #[structopt(long, default_value = "changeme")]
    /// Password for the wallet database
    wallet_pass: String,

    #[structopt(short, long, default_value = "tcp://127.0.0.1:8340")]
    /// darkfid JSON-RPC endpoint
    endpoint: Url,

    #[structopt(subcommand)]
    /// Sub command to execute
    command: Subcmd,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

#[derive(Clone, Debug, Deserialize, StructOpt)]
enum Subcmd {
    /// Fun
    Kaching,

    /// Send a ping request to the darkfid RPC endpoint
    Ping,

    // TODO: shell completions
    /// Wallet operations
    Wallet {
        #[structopt(long)]
        /// Initialize wallet database
        initialize: bool,

        #[structopt(long)]
        /// Generate a new keypair in the wallet
        keygen: bool,

        #[structopt(long)]
        /// Query the wallet for known balances
        balance: bool,

        #[structopt(long)]
        /// Get the default address in the wallet
        address: bool,

        #[structopt(long)]
        /// Print all the secret keys from the wallet
        secrets: bool,

        #[structopt(long)]
        /// Import secret keys from stdin into the wallet, separated by newlines
        import_secrets: bool,

        #[structopt(long)]
        /// Print the Merkle tree in the wallet
        tree: bool,

        #[structopt(long)]
        /// Print all the coins in the wallet
        coins: bool,
    },
}

/// CLI-util structure
pub struct Drk {
    /// Wallet database operations handler
    pub wallet: WalletPtr,
    /// JSON-RPC client to execute requests to darkfid daemon
    pub rpc_client: RpcClient,
}

impl Drk {
    async fn new(
        wallet_path: String,
        wallet_pass: String,
        endpoint: Url,
        ex: Arc<smol::Executor<'static>>,
    ) -> Result<Self> {
        let wallet = match WalletDb::new(Some(expand_path(&wallet_path)?), Some(&wallet_pass)) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Error initializing wallet: {e:?}");
                exit(2);
            }
        };
        let rpc_client = RpcClient::new(endpoint, ex).await?;
        Ok(Self { wallet, rpc_client })
    }

    /// Auxilliary function to ping configured darkfid daemon for liveness.
    async fn ping(&self) -> Result<()> {
        eprintln!("Executing ping request to darkfid...");
        let latency = Instant::now();
        let req = JsonRequest::new("ping", JsonValue::Array(vec![]));
        let rep = self.rpc_client.oneshot_request(req).await?;
        let latency = latency.elapsed();
        eprintln!("Got reply: {:?}", rep);
        eprintln!("Latency: {:?}", latency);
        Ok(())
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    match args.command {
        Subcmd::Kaching => {
            kaching().await;
            Ok(())
        }

        Subcmd::Ping => {
            let drk = Drk::new(args.wallet_path, args.wallet_pass, args.endpoint, ex).await?;
            drk.ping().await
        }

        Subcmd::Wallet {
            initialize,
            keygen,
            balance,
            address,
            secrets,
            import_secrets,
            tree,
            coins,
        } => {
            if !initialize &&
                !keygen &&
                !balance &&
                !address &&
                !secrets &&
                !tree &&
                !coins &&
                !import_secrets
            {
                eprintln!("Error: You must use at least one flag for this subcommand");
                eprintln!("Run with \"wallet -h\" to see the subcommand usage.");
                exit(2);
            }

            // TODO

            Ok(())
        }
    }
}
