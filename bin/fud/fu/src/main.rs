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
use std::{collections::HashMap, sync::Arc};
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
        /// File hash
        file: String,
        /// File name
        name: Option<String>,
    },

    /// Put a file onto the fud network
    Put {
        /// File name
        file: String,
    },

    /// Get the current node buckets
    ListBuckets {},

    /// Get the router state
    ListSeeders {},
}

struct Fu {
    pub rpc_client: RpcClient,
}

impl Fu {
    async fn close_connection(&self) {
        self.rpc_client.stop().await;
    }

    async fn get(&self, file_hash: String, file_name: Option<String>) -> Result<()> {
        let req = JsonRequest::new("get", JsonValue::Array(vec![JsonValue::String(file_hash), JsonValue::String(file_name.unwrap_or_default())]));
        let rep = self.rpc_client.request(req).await?;
        let path: String = rep.try_into().unwrap();
        println!("{}", path);
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

    async fn list_buckets(&self) -> Result<()> {
        let req = JsonRequest::new("list_buckets", JsonValue::Array(vec![]));
        let rep = self.rpc_client.request(req).await?;
        let buckets: Vec<JsonValue> = rep.try_into().unwrap();
        for (bucket_i, bucket) in buckets.into_iter().enumerate() {
            let nodes: Vec<JsonValue> = bucket.try_into().unwrap();
            if nodes.len() == 0 {
                continue
            }

            println!("Bucket {}", bucket_i);
            for n in nodes.clone() {
                let node: Vec<JsonValue> = n.try_into().unwrap();
                let node_id: JsonValue = node[0].clone();
                let addresses: Vec<JsonValue> = node[1].clone().try_into().unwrap();
                let mut addrs: Vec<String> = vec![];
                for addr in addresses {
                    addrs.push(addr.try_into().unwrap())
                }
                println!("\t{}: {}", node_id.stringify().unwrap(), addrs.join(", "));
            }
        }

        Ok(())
    }

    async fn list_seeders(&self) -> Result<()> {
        let req = JsonRequest::new("list_seeders", JsonValue::Array(vec![]));
        let rep = self.rpc_client.request(req).await?;

        let files: HashMap<String, JsonValue> = rep["seeders"].clone().try_into().unwrap();

        println!("Seeders:");
        if files.is_empty() {
            println!("No records");
        } else {
            for (file_hash, node_ids) in files {
                println!("{}", file_hash);
                let node_ids: Vec<JsonValue> = node_ids.try_into().unwrap();
                for node_id in node_ids {
                    let node_id: String = node_id.try_into().unwrap();
                    println!("\t{}", node_id);
                }
            }
        }

        Ok(())
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
                Subcmd::Get { file, name } => fu.get(file, name).await,
                Subcmd::Put { file } => fu.put(file).await,
                Subcmd::ListBuckets { } => fu.list_buckets().await,
                Subcmd::ListSeeders { } => fu.list_seeders().await,
            }?;

            fu.close_connection().await;

            Ok(())
        })
        .await
    })
}
