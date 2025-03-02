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

use clap::{Parser, Subcommand};
use log::info;
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use std::sync::Arc;
use url::Url;

use darkfi::{
    cli_desc,
    rpc::{client::RpcClient, jsonrpc::JsonRequest, util::JsonValue},
    util::cli::{get_log_config, get_log_level},
    Error, Result,
};

#[derive(Parser)]
#[clap(name = "fu", about = cli_desc!(), version)]
#[clap(arg_required_else_help(true))]
struct Args {
    #[clap(short, action = clap::ArgAction::Count)]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[clap(short, long, default_value = "tcp://127.0.0.1:13336")]
    /// fud JSON-RPC endpoint
    endpoint: Url,

    #[clap(subcommand)]
    command: Subcmd,
}

#[derive(Subcommand)]
enum Subcmd {
    /// Retrieve provided file name from the fud network
    Get {
        /// File name
        file: String,
    },

    /// Put a file onto the fud network
    Put {
        /// File name
        file: String,
    },
}

struct Fu {
    pub rpc_client: RpcClient,
}

impl Fu {
    async fn close_connection(&self) {
        self.rpc_client.stop().await;
    }

    // async fn list(&self) -> Result<()> {
    //     let req = JsonRequest::new("list", JsonValue::Array(vec![]));
    //     let rep = self.rpc_client.request(req).await?;
    //
    //     // Extract response
    //     let content: Vec<JsonValue> = rep[0].clone().try_into().unwrap();
    //     let new: Vec<JsonValue> = rep[1].clone().try_into().unwrap();
    //     let deleted: Vec<JsonValue> = rep[2].clone().try_into().unwrap();
    //
    //     // Print info
    //     info!("----------Content-------------");
    //     if content.is_empty() {
    //         info!("No file records exists in DHT.");
    //     } else {
    //         for name in content {
    //             info!("\t{}", String::try_from(name).unwrap());
    //         }
    //     }
    //     info!("------------------------------");
    //
    //     info!("----------New files-----------");
    //     if new.is_empty() {
    //         info!("No new files to import.");
    //     } else {
    //         for name in new {
    //             info!("\t{}", String::try_from(name).unwrap());
    //         }
    //     }
    //     info!("------------------------------");
    //
    //     info!("----------Removed keys--------");
    //     if deleted.is_empty() {
    //         info!("No keys were removed.");
    //     } else {
    //         for key in deleted {
    //             info!("\t{}", String::try_from(key).unwrap());
    //         }
    //     }
    //     info!("------------------------------");
    //
    //     Ok(())
    // }
    //
    // async fn sync(&self) -> Result<()> {
    //     let req = JsonRequest::new("sync", JsonValue::Array(vec![]));
    //     self.rpc_client.request(req).await?;
    //     info!("Daemon synced successfully!");
    //     Ok(())
    // }

    async fn get(&self, file: String) -> Result<()> {
        let req = JsonRequest::new("get", JsonValue::Array(vec![JsonValue::String(file)]));
        let rep = self.rpc_client.request(req).await?;
        let path = rep.stringify().unwrap();
        info!("File waits you at: {}", path);
        Ok(())
    }

    async fn put(&self, file: String) -> Result<()> {
        let req = JsonRequest::new("put", JsonValue::Array(vec![JsonValue::String(file)]));
        let rep = self.rpc_client.request(req).await?;
        match rep {
            JsonValue::String(file_id) => {
                println!("{}", file_id);
                Ok(())
            }
            _ => Err(Error::ParseFailed("File ID is not a string")),
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = get_log_level(args.verbose);
    let log_config = get_log_config(args.verbose);
    TermLogger::init(log_level, log_config, TerminalMode::Mixed, ColorChoice::Auto)?;

    let ex = Arc::new(smol::Executor::new());
    smol::block_on(async {
        ex.run(async {
            let rpc_client = RpcClient::new(args.endpoint, ex.clone()).await?;
            let fu = Fu { rpc_client };

            match args.command {
                // Subcmd::List => fu.list().await,
                // Subcmd::Sync => fu.sync().await,
                Subcmd::Get { file } => fu.get(file).await,
                Subcmd::Put { file } => fu.put(file).await,
            }?;

            fu.close_connection().await;

            Ok(())
        })
        .await
    })
}
