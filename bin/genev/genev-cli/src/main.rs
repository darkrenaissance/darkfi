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

use std::sync::Arc;

use clap::{Parser, Subcommand};
use darkfi::{
    rpc::client::RpcClient,
    util::cli::{get_log_config, get_log_level},
    Result,
};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use smol::Executor;
use url::Url;

use genevd::GenEvent;

mod rpc;
use rpc::Gen;

#[derive(Parser)]
#[clap(name = "genev", version)]
struct Args {
    #[arg(short, action = clap::ArgAction::Count)]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[clap(short, long, default_value = "tcp://127.0.0.1:28880")]
    /// JSON-RPC endpoint
    endpoint: Url,

    #[clap(subcommand)]
    command: Option<SubCmd>,
}

#[derive(Subcommand)]
enum SubCmd {
    Add { values: Vec<String> },

    List,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = get_log_level(args.verbose);
    let log_config = get_log_config(args.verbose);
    TermLogger::init(log_level, log_config, TerminalMode::Mixed, ColorChoice::Auto)?;

    let executor = Arc::new(Executor::new());

    smol::block_on(executor.run(async {
        let rpc_client = RpcClient::new(args.endpoint, executor.clone()).await?;
        let gen = Gen { rpc_client };

        match args.command {
            Some(subcmd) => match subcmd {
                SubCmd::Add { values } => {
                    let event = GenEvent {
                        nick: values[0].clone(),
                        title: values[1].clone(),
                        text: values[2..].join(" "),
                    };

                    return gen.add(event).await
                }

                SubCmd::List => {
                    let events = gen.list().await?;
                    for event in events {
                        println!("=============================");
                        println!(
                            "- nickname: {}, title: {}, text: {}",
                            event.nick, event.title, event.text
                        );
                    }
                }
            },
            None => println!("none"),
        }

        gen.close_connection().await;

        Ok(())
    }))
}
