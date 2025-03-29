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
use log::error;
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use std::{
    collections::HashMap,
    io::{stdout, Write},
    sync::Arc,
};
use url::Url;

use darkfi::{
    cli_desc,
    rpc::{
        client::RpcClient,
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult},
        util::JsonValue,
    },
    system::{ExecutorPtr, Publisher, StoppableTask},
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
    pub rpc_client: Arc<RpcClient>,
}

impl Fu {
    async fn get(
        &self,
        file_hash: String,
        file_name: Option<String>,
        ex: ExecutorPtr,
    ) -> Result<()> {
        let publisher = Publisher::new();
        let subscription = Arc::new(publisher.clone().subscribe().await);
        let subscriber_task = StoppableTask::new();
        let file_hash_ = file_hash.clone();
        let publisher_ = publisher.clone();
        let rpc_client_ = self.rpc_client.clone();
        subscriber_task.clone().start(
            async move {
                let req = JsonRequest::new("subscribe", JsonValue::Array(vec![]));
                rpc_client_.subscribe(req, publisher).await
            },
            move |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => {
                        error!("{}", e);
                        publisher_
                            .notify(JsonResult::Error(JsonError::new(
                                ErrorCode::InternalError,
                                None,
                                0,
                            )))
                            .await;
                    }
                }
            },
            Error::DetachedTaskStopped,
            ex,
        );

        let progress_bar_width = 20;
        let mut chunks_total = 0;
        let mut chunks_downloaded = 0;

        let print_progress_bar = |chunks_downloaded: usize, chunks_total: usize| {
            let completed = (chunks_downloaded as f64 / chunks_total as f64 *
                progress_bar_width as f64) as usize;
            let remaining = progress_bar_width - completed;
            let bar = "=".repeat(completed) + &" ".repeat(remaining);
            print!("\r[{}] {}/{} chunks", bar, chunks_downloaded, chunks_total);
            stdout().flush().unwrap();
        };

        let req = JsonRequest::new(
            "get",
            JsonValue::Array(vec![
                JsonValue::String(file_hash_.clone()),
                JsonValue::String(file_name.unwrap_or_default()),
            ]),
        );
        let _ = self.rpc_client.request(req).await;

        loop {
            match subscription.receive().await {
                JsonResult::Notification(n) => {
                    let params = n.params.get::<HashMap<String, JsonValue>>().unwrap();
                    let info =
                        params.get("info").unwrap().get::<HashMap<String, JsonValue>>().unwrap();
                    let hash = info.get("file_hash").unwrap().get::<String>().unwrap();
                    if *hash != file_hash_ {
                        continue;
                    }
                    match params.get("event").unwrap().get::<String>().unwrap().as_str() {
                        "file_download_completed" => {
                            chunks_total =
                                *info.get("chunk_count").unwrap().get::<f64>().unwrap() as usize;
                            print_progress_bar(chunks_downloaded, chunks_total);
                        }
                        "chunk_download_completed" => {
                            chunks_downloaded += 1;
                            print_progress_bar(chunks_downloaded, chunks_total);
                        }
                        "download_completed" => {
                            let info = params
                                .get("info")
                                .unwrap()
                                .get::<HashMap<String, JsonValue>>()
                                .unwrap();
                            let file_path = info.get("file_path").unwrap().get::<String>().unwrap();
                            chunks_downloaded = chunks_total;
                            print_progress_bar(chunks_downloaded, chunks_total);
                            println!("\nDownload completed:\n{}", file_path);
                            return Ok(());
                        }
                        "file_not_found" => {
                            return Err(Error::Custom(format!("Could not find file {}", file_hash)));
                        }
                        "chunk_not_found" => {
                            // A seeder does not have a chunk we are looking for,
                            // we will try another seeder so there is nothing to do
                        }
                        "missing_chunks" => {
                            // We tried all seeders and some chunks are still missing
                            println!();
                            return Err(Error::Custom("Missing chunks".to_string()));
                        }
                        "download_error" => {
                            // An error that caused the download to be unsuccessful
                            let info = params
                                .get("info")
                                .unwrap()
                                .get::<HashMap<String, JsonValue>>()
                                .unwrap();
                            println!();
                            return Err(Error::Custom(
                                info.get("error").unwrap().get::<String>().unwrap().to_string(),
                            ));
                        }
                        _ => {}
                    }
                }

                JsonResult::Error(e) => {
                    return Err(Error::UnexpectedJsonRpc(format!("Got error from JSON-RPC: {e:?}")))
                }

                x => {
                    return Err(Error::UnexpectedJsonRpc(format!(
                        "Got unexpected data from JSON-RPC: {x:?}"
                    )))
                }
            }
        }
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
        let mut empty = true;
        for (bucket_i, bucket) in buckets.into_iter().enumerate() {
            let nodes: Vec<JsonValue> = bucket.try_into().unwrap();
            if nodes.is_empty() {
                continue
            }
            empty = false;

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

        if empty {
            println!("All buckets are empty");
        }

        Ok(())
    }

    async fn list_seeders(&self) -> Result<()> {
        let req = JsonRequest::new("list_seeders", JsonValue::Array(vec![]));
        let rep = self.rpc_client.request(req).await?;

        let files: HashMap<String, JsonValue> = rep["seeders"].clone().try_into().unwrap();

        if files.is_empty() {
            println!("No known seeders");
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
            let rpc_client = Arc::new(RpcClient::new(args.endpoint.clone(), ex.clone()).await?);
            let fu = Fu { rpc_client };

            match args.command {
                Subcmd::Get { file, name } => fu.get(file, name, ex.clone()).await,
                Subcmd::Put { file } => fu.put(file).await,
                Subcmd::ListBuckets {} => fu.list_buckets().await,
                Subcmd::ListSeeders {} => fu.list_seeders().await,
            }?;

            Ok(())
        })
        .await
    })
}
