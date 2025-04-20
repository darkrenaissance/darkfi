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
use smol::lock::RwLock;
use std::{
    collections::HashMap,
    io::{stdout, Write},
    sync::Arc,
};
use termcolor::{Color, ColorSpec, StandardStream, WriteColor};
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

    /// Watch
    Watch {},

    /// Remove a resource from fud
    Rm {
        /// Resource hash
        hash: String,
    },

    /// Get the current node buckets
    ListBuckets {},

    /// Get the router state
    ListSeeders {},
}

struct Fu {
    pub rpc_client: Arc<RpcClient>,
    pub endpoint: Url,
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
            ex.clone(),
        );

        let progress_bar_width = 20;

        let print_progress_bar = |info: &HashMap<String, JsonValue>| {
            let resource =
                info.get("resource").unwrap().get::<HashMap<String, JsonValue>>().unwrap();
            let chunks_downloaded =
                *resource.get("chunks_downloaded").unwrap().get::<f64>().unwrap() as usize;
            let chunks_total =
                *resource.get("chunks_total").unwrap().get::<f64>().unwrap() as usize;
            let status = match resource.get("status").unwrap().get::<String>().unwrap().as_str() {
                "seeding" => "done",
                s => s,
            };
            let completed = if chunks_total > 0 {
                (chunks_downloaded as f64 / chunks_total as f64 * progress_bar_width as f64)
                    as usize
            } else {
                0
            };
            let remaining = progress_bar_width - completed;
            let bar = "=".repeat(completed) + &" ".repeat(remaining);
            print!("\x1B[2K\r[{}] {}/{} chunks | {}", bar, chunks_downloaded, chunks_total, status);
            stdout().flush().unwrap();
        };

        let req = JsonRequest::new(
            "get",
            JsonValue::Array(vec![
                JsonValue::String(file_hash_.clone()),
                JsonValue::String(file_name.unwrap_or_default()),
            ]),
        );
        // Create a RPC client to send the `get` request
        let rpc_client_getter = RpcClient::new(self.endpoint.clone(), ex.clone()).await?;
        let _ = rpc_client_getter.request(req).await?;

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
                        "download_started" |
                        "file_download_completed" |
                        "chunk_download_completed" => {
                            print_progress_bar(info);
                        }
                        "download_completed" => {
                            let file_path = info.get("file_path").unwrap().get::<String>().unwrap();
                            print_progress_bar(info);
                            println!("\nDownload completed:\n{}", file_path);
                            return Ok(());
                        }
                        "file_not_found" => {
                            println!();
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

    async fn watch(&self, ex: ExecutorPtr) -> Result<()> {
        let req = JsonRequest::new("list_resources", JsonValue::Array(vec![]));
        let rep = self.rpc_client.request(req).await?;

        let resources_json: Vec<JsonValue> = rep.clone().try_into().unwrap();
        let resources: Arc<RwLock<Vec<HashMap<String, JsonValue>>>> = Arc::new(RwLock::new(vec![]));

        let publisher = Publisher::new();
        let subscription = Arc::new(publisher.clone().subscribe().await);
        let subscriber_task = StoppableTask::new();
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

        let mut tstdout = StandardStream::stdout(ColorChoice::Auto);

        let mut update_resource = async |resource: &HashMap<String, JsonValue>| {
            let hash = resource.get("hash").unwrap().get::<String>().unwrap();
            let mut resources_write = resources.write().await;
            let i = match resources_write
                .iter()
                .position(|r| r.get("hash").unwrap().get::<String>().unwrap() == hash)
            {
                Some(i) => {
                    resources_write.remove(i);
                    resources_write.insert(i, resource.clone());
                    i
                }
                None => {
                    resources_write.push(resource.clone());
                    resources_write.len() - 1
                }
            };

            // Move the cursor to the i-th line and clear it
            print!("\x1b[{};1H\x1B[2K", i + 2);

            let hash = resource.get("hash").unwrap().get::<String>().unwrap();
            print!("\r{:>44} ", hash,);

            let status = resource.get("status").unwrap().get::<String>().unwrap();
            tstdout
                .set_color(
                    ColorSpec::new()
                        .set_fg(match status.as_str() {
                            "downloading" => Some(Color::Blue),
                            "seeding" => Some(Color::Green),
                            "discovering" => Some(Color::Magenta),
                            "incomplete" => Some(Color::Red),
                            _ => None,
                        })
                        .set_bold(true),
                )
                .unwrap();
            print!("{:>11} ", status,);
            tstdout.reset().unwrap();

            let chunks_downloaded =
                *resource.get("chunks_downloaded").unwrap().get::<f64>().unwrap() as usize;
            let chunks_total =
                *resource.get("chunks_total").unwrap().get::<f64>().unwrap() as usize;
            match chunks_total {
                0 => {
                    print!("{:>5.1} {:>9}", 0.0, format!("{}/?", chunks_downloaded));
                }
                _ => {
                    let percent = chunks_downloaded as f64 / chunks_total as f64 * 100.0;
                    print!(
                        "{:>5.1} {:>9}",
                        percent,
                        format!("{}/{}", chunks_downloaded, chunks_total)
                    );
                }
            };
            println!();

            // Move the cursor to end
            print!("\x1b[{};1H", resources_write.len() + 2);
            stdout().flush().unwrap();
        };

        let print_begin = async || {
            // Clear
            print!("\x1B[2J\x1B[1;1H");

            // Print column headers
            println!("\x1b[4m{:>44} {:>11} {:>5} {:>9}\x1b[0m", "Hash", "Status", "%", "Chunks");
        };

        print_begin().await;
        if resources_json.is_empty() {
            println!("No known resources");
        } else {
            for resource in resources_json.iter() {
                let resource = resource.get::<HashMap<String, JsonValue>>().unwrap();
                update_resource(resource).await;
            }
        }

        loop {
            match subscription.receive().await {
                JsonResult::Notification(n) => {
                    let params = n.params.get::<HashMap<String, JsonValue>>().unwrap();
                    let info =
                        params.get("info").unwrap().get::<HashMap<String, JsonValue>>().unwrap();
                    match params.get("event").unwrap().get::<String>().unwrap().as_str() {
                        "download_started" |
                        "file_download_completed" |
                        "chunk_download_completed" |
                        "download_completed" |
                        "missing_chunks" |
                        "file_not_found" => {
                            let resource = info
                                .get("resource")
                                .unwrap()
                                .get::<HashMap<String, JsonValue>>()
                                .unwrap();
                            update_resource(resource).await;
                        }
                        "resource_removed" => {
                            {
                                let hash = info.get("file_hash").unwrap().get::<String>().unwrap();
                                let mut resources_write = resources.write().await;
                                let i = resources_write.iter().position(|r| {
                                    r.get("hash").unwrap().get::<String>().unwrap() == hash
                                });
                                if let Some(i) = i {
                                    resources_write.remove(i);
                                }
                            }

                            let r = resources.read().await.clone();
                            print_begin().await;
                            for resource in r.iter() {
                                update_resource(resource).await;
                            }
                        }
                        "download_error" => {
                            // An error that caused the download to be unsuccessful
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

    async fn remove(&self, hash: String) -> Result<()> {
        let req = JsonRequest::new("remove", JsonValue::Array(vec![JsonValue::String(hash)]));
        self.rpc_client.request(req).await?;
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
            let fu = Fu { rpc_client, endpoint: args.endpoint.clone() };

            match args.command {
                Subcmd::Get { file, name } => fu.get(file, name, ex.clone()).await,
                Subcmd::Put { file } => fu.put(file).await,
                Subcmd::Watch {} => fu.watch(ex.clone()).await,
                Subcmd::Rm { hash } => fu.remove(hash).await,
                Subcmd::ListBuckets {} => fu.list_buckets().await,
                Subcmd::ListSeeders {} => fu.list_seeders().await,
            }?;

            Ok(())
        })
        .await
    })
}
