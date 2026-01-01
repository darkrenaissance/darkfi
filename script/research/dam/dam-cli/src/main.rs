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

use std::sync::Arc;

use clap::{Parser, Subcommand};
use darkfi::{cli_desc, rpc::util::JsonValue, util::logger::setup_logging, Result};
use smol::Executor;

use dam_cli::DamCli;

#[derive(Parser)]
#[command(about = cli_desc!())]
struct Args {
    #[arg(short, long, default_value = "tcp://127.0.0.1:34780")]
    /// damd JSON-RPC endpoint
    endpoint: String,

    #[command(subcommand)]
    /// Sub command to execute
    command: Subcmd,
}

#[derive(Subcommand)]
enum Subcmd {
    /// Send a ping request to the damd RPC endpoint
    Ping,

    /// This subscription will listen for incoming notifications from damd
    Subscribe {
        /// The method to subscribe to
        method: String,
    },

    /// Signal damd to execute a flooding attack against the network
    Flood {
        /// Optional flood messages count limit
        limit: Option<u32>,
    },

    /// Signal damd to stop an ongoing flooding attack
    StopFlood,
}

fn main() -> Result<()> {
    // Setup terminal logger. verbosity level 0 == Level::Info
    setup_logging(0, None)?;

    // Initialize an executor
    let executor = Arc::new(Executor::new());
    let ex = executor.clone();
    smol::block_on(executor.run(async {
        // Parse arguments
        let args = Args::parse();

        // Execute a subcommand
        let dam_cli = DamCli::new(&args.endpoint, &ex).await?;
        match args.command {
            Subcmd::Ping => {
                dam_cli.ping().await?;
            }

            Subcmd::Subscribe { method } => {
                dam_cli.subscribe(&args.endpoint, &method, &ex).await?;
            }

            Subcmd::Flood { limit } => {
                let limit = match limit {
                    Some(l) => JsonValue::String(format!("{l}")),
                    None => JsonValue::String(String::from("0")),
                };
                dam_cli
                    .damd_daemon_request(
                        "flood.switch",
                        &JsonValue::Array(vec![JsonValue::Boolean(true), limit]),
                    )
                    .await?;
            }

            Subcmd::StopFlood => {
                dam_cli
                    .damd_daemon_request(
                        "flood.switch",
                        &JsonValue::Array(vec![
                            JsonValue::Boolean(false),
                            JsonValue::String(String::from("0")),
                        ]),
                    )
                    .await?;
            }
        }
        dam_cli.rpc_client.stop().await;

        Ok(())
    }))
}
