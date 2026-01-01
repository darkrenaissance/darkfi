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
use darkfi::{cli_desc, Result};
use prettytable::{format, row, Table};
use smol::Executor;

use rlnd_cli::RlndCli;

#[derive(Parser)]
#[command(about = cli_desc!())]
struct Args {
    #[arg(short, long, default_value = "tcp://127.0.0.1:25637")]
    /// rldn JSON-RPC endpoint
    endpoint: String,

    #[command(subcommand)]
    /// Sub command to execute
    command: Subcmd,
}

#[derive(Subcommand)]
enum Subcmd {
    /// Send a ping request to the rlnd RPC endpoint
    Ping,

    /// List all memberships
    List,

    /// Register a membership
    Register {
        /// Stake of this membership
        stake: u64,
    },

    /// Slash a membership
    Slash {
        /// Membership id to slash
        id: String,
    },
}

fn main() -> Result<()> {
    // Initialize an executor
    let executor = Arc::new(Executor::new());
    let ex = executor.clone();
    smol::block_on(executor.run(async {
        // Parse arguments
        let args = Args::parse();

        // Execute a subcommand
        let rlnd_cli = RlndCli::new(&args.endpoint, ex).await?;
        match args.command {
            Subcmd::Ping => {
                rlnd_cli.ping().await?;
            }

            Subcmd::List => {
                match rlnd_cli.get_all_memberships().await {
                    Ok(memberships) => {
                        // Create a prettytable with the memberships:
                        let mut table = Table::new();
                        table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
                        table.set_titles(row!["ID", "Leaf Position", "Stake"]);
                        for (id, membership) in memberships.iter() {
                            table.add_row(row![
                                id,
                                format!("{:?}", membership.leaf_position),
                                membership.stake
                            ]);
                        }

                        if table.is_empty() {
                            println!("No memberships found");
                        } else {
                            println!("{table}");
                        }
                    }
                    Err(e) => println!("Membership registration failed: {e}"),
                }
            }

            Subcmd::Register { stake } => match rlnd_cli.register_membership(stake).await {
                Ok((id, membership)) => println!("Registered membership {id:?}: {membership:?}"),
                Err(e) => println!("Membership registration failed: {e}"),
            },

            Subcmd::Slash { id } => {
                println!("Slashing membership: {id}");
                match rlnd_cli.slash_membership(&id).await {
                    Ok(membership) => println!("Slashed membership {id}: {membership:?}"),
                    Err(e) => println!("Membership slashing failed: {e}"),
                }
            }
        }
        rlnd_cli.rpc_client.stop().await;

        Ok(())
    }))
}
