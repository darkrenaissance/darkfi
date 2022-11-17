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

use std::{process::exit, time::Instant};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde_json::json;
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use url::Url;

use darkfi::{
    cli_desc,
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
    util::cli::{get_log_config, get_log_level},
};

/// Wallet operation methods for darkfid's JSON-RPC
mod rpc_wallet;

#[derive(Parser)]
#[command(about = cli_desc!())]
struct Args {
    #[arg(short, action = clap::ArgAction::Count)]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[arg(short, long, default_value = "tcp://127.0.0.1:8340")]
    /// darkfid JSON-RPC endpoint
    endpoint: Url,

    #[command(subcommand)]
    command: Subcmd,
}

#[derive(Subcommand)]
enum Subcmd {
    /// Send a ping request to the darkfid RPC endpoint
    Ping,

    /// Wallet operations
    Wallet {
        #[arg(long)]
        /// Initialize wallet with data for Money Contract (run this first)
        initialize: bool,

        #[arg(long)]
        /// Generate a new keypair in the wallet
        keygen: bool,

        #[arg(long)]
        /// Query the wallet for known balances
        balance: bool,

        #[arg(long)]
        /// Get the default address in the wallet
        address: bool,
    },
}

pub struct Drk {
    pub rpc_client: RpcClient,
}

impl Drk {
    async fn ping(&self) -> Result<()> {
        let latency = Instant::now();
        let req = JsonRequest::new("ping", json!([]));
        let rep = self.rpc_client.oneshot_request(req).await?;
        let latency = latency.elapsed();
        println!("Got reply: {}", rep);
        println!("Latency: {:?}", latency);
        Ok(())
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.verbose > 0 {
        let log_level = get_log_level(args.verbose.into());
        let log_config = get_log_config();
        TermLogger::init(log_level, log_config, TerminalMode::Mixed, ColorChoice::Auto)?;
    }

    match args.command {
        Subcmd::Ping => {
            let rpc_client = RpcClient::new(args.endpoint)
                .await
                .with_context(|| "Could not connect to RPC endpoint")?;

            let drk = Drk { rpc_client };
            drk.ping().await.with_context(|| "Failed to ping RPC endpoint")?;
            Ok(())
        }

        Subcmd::Wallet { initialize, keygen, balance, address } => {
            if !initialize && !keygen && !balance && !address {
                eprintln!("Error: You must use at least one flag for this subcommand");
                eprintln!("Run with \"wallet -h\" to see the subcommand usage.");
                exit(2);
            }

            let rpc_client = RpcClient::new(args.endpoint)
                .await
                .with_context(|| "Could not connect to RPC endpoint")?;

            let drk = Drk { rpc_client };

            if initialize {
                drk.wallet_initialize().await.with_context(|| "Failed to initialize wallet")?;
                return Ok(())
            }

            if keygen {
                drk.wallet_keygen().await.with_context(|| "Failed to generate keypair")?;
                return Ok(())
            }

            if balance {
                drk.wallet_balance().await.with_context(|| "Failed to fetch wallet balance")?;
                return Ok(())
            }

            if address {
                drk.wallet_address(0).await.with_context(|| "Failed to fetch default address")?;
                return Ok(())
            }

            unreachable!()
        }
    }
}
