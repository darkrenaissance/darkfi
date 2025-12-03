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
use smol::lock::RwLock;
use std::{
    collections::HashMap,
    io::{stdout, Write},
    sync::Arc,
};
use termcolor::{ColorChoice, StandardStream, WriteColor};
use tracing::error;
use url::Url;

use darkfi::{
    cli_desc,
    rpc::{
        client::RpcClient,
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult},
        util::JsonValue,
    },
    system::{ExecutorPtr, Publisher, StoppableTask},
    util::logger::setup_logging,
    Error, Result,
};

use fud::{
    resource::{Resource, ResourceStatus, ResourceType},
    util::hash_to_string,
};

mod util;
use crate::util::{
    format_bytes, format_duration, format_progress_bytes, optional_value, print_tree,
    status_to_colorspec, type_to_colorspec, TreeNode,
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
    /// Retrieve provided resource from the fud network
    Get {
        /// Resource hash
        hash: String,
        /// Download path (relative or absolute)
        path: Option<String>,
        /// Optional list of files you want to download (only used for directories)
        #[arg(short, long, num_args = 1..)]
        files: Option<Vec<String>>,
    },

    /// Put a file or directory onto the fud network
    Put {
        /// File path or directory path
        path: String,
    },

    /// List resources
    Ls {},

    /// Watch
    Watch {},

    /// Remove a resource from fud
    Rm {
        /// Resource hash
        hash: String,
    },

    /// Get the current node buckets
    Buckets {},

    /// Get the router state
    Seeders {},

    /// Verify local files
    Verify {
        /// File hashes
        files: Option<Vec<String>>,
    },

    /// Lookup seeders of a resource from the network
    Lookup {
        /// Resource hash
        hash: String,
    },
}

struct Fu {
    pub rpc_client: Arc<RpcClient>,
    pub endpoint: Url,
}

impl Fu {
    async fn get(
        &self,
        hash: String,
        path: Option<String>,
        files: Option<Vec<String>>,
        ex: ExecutorPtr,
    ) -> Result<()> {
        let publisher = Publisher::new();
        let subscription = Arc::new(publisher.clone().subscribe().await);
        let subscriber_task = StoppableTask::new();
        let hash_ = hash.clone();
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
                        error!("{e}");
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
        let mut started = false;
        let mut tstdout = StandardStream::stdout(ColorChoice::Auto);

        let mut print_progress = |info: &HashMap<String, JsonValue>| {
            started = true;
            let rs: Resource = info.get("resource").unwrap().clone().into();

            print!("\x1B[2K\r"); // Clear current line

            // Progress bar
            let percent = if rs.target_bytes_downloaded > rs.target_bytes_size {
                1.0
            } else if rs.target_bytes_size > 0 {
                rs.target_bytes_downloaded as f64 / rs.target_bytes_size as f64
            } else {
                0.0
            };
            let completed = (percent * progress_bar_width as f64) as usize;
            let remaining = match progress_bar_width > completed {
                true => progress_bar_width - completed,
                false => 0,
            };
            let bar = "=".repeat(completed) + &" ".repeat(remaining);
            print!("[{bar}] {:.1}% | ", percent * 100.0);

            // Downloaded / Total (in bytes)
            if rs.target_bytes_size > 0 {
                if rs.target_bytes_downloaded == rs.target_bytes_size {
                    print!("{} | ", format_bytes(rs.target_bytes_size));
                } else {
                    print!(
                        "{} | ",
                        format_progress_bytes(rs.target_bytes_downloaded, rs.target_bytes_size)
                    );
                }
            }

            // Download speed (in bytes/sec)
            if !rs.speeds.is_empty() && rs.target_chunks_downloaded < rs.target_chunks_count {
                print!("{}/s | ", format_bytes(*rs.speeds.last().unwrap() as u64));
            }

            // Downloaded / Total (in chunks)
            if rs.target_chunks_count > 0 {
                let s = if rs.target_chunks_count > 1 { "s" } else { "" };
                if rs.target_chunks_downloaded == rs.target_chunks_count {
                    print!("{} chunk{s} | ", rs.target_chunks_count);
                } else {
                    print!(
                        "{}/{} chunk{s} | ",
                        rs.target_chunks_downloaded, rs.target_chunks_count
                    );
                }
            }

            // ETA
            if !rs.speeds.is_empty() && rs.target_chunks_downloaded < rs.target_chunks_count {
                print!("ETA: {} | ", format_duration(rs.get_eta()));
            }

            // Status
            let is_done = rs.target_chunks_downloaded == rs.target_chunks_count &&
                rs.status.as_str() == "incomplete";
            let status = if is_done { ResourceStatus::Seeding } else { rs.status };
            tstdout.set_color(&status_to_colorspec(&status)).unwrap();
            print!(
                "{}",
                if let ResourceStatus::Seeding = status { "done" } else { status.as_str() }
            );
            tstdout.reset().unwrap();
            stdout().flush().unwrap();
        };

        let req = JsonRequest::new(
            "get",
            JsonValue::Array(vec![
                JsonValue::String(hash_.clone()),
                JsonValue::String(path.unwrap_or_default()),
                match files {
                    Some(files) => {
                        JsonValue::Array(files.into_iter().map(JsonValue::String).collect())
                    }
                    None => JsonValue::Null,
                },
            ]),
        );
        // Create a RPC client to send the `get` request
        let rpc_client_getter = RpcClient::new(self.endpoint.clone(), ex.clone()).await?;
        let _ = rpc_client_getter.request(req).await?;

        loop {
            match subscription.receive().await {
                JsonResult::Notification(n) => {
                    let params = n.params.get::<HashMap<String, JsonValue>>().unwrap();
                    let info = params.get("info");
                    if info.is_none() {
                        continue
                    }
                    let info = info.unwrap().get::<HashMap<String, JsonValue>>().unwrap();

                    let hash = match info.get("hash") {
                        Some(hash_value) => hash_value.get::<String>().unwrap(),
                        None => continue,
                    };
                    if *hash != hash_ {
                        continue;
                    }
                    match params.get("event").unwrap().get::<String>().unwrap().as_str() {
                        "download_started" |
                        "metadata_download_completed" |
                        "chunk_download_completed" |
                        "resource_updated" => {
                            print_progress(info);
                        }
                        "download_completed" => {
                            let resource_json = info
                                .get("resource")
                                .unwrap()
                                .get::<HashMap<String, JsonValue>>()
                                .unwrap();
                            let path = resource_json.get("path").unwrap().get::<String>().unwrap();
                            print_progress(info);
                            println!("\nDownload completed:\n{path}");
                            return Ok(());
                        }
                        "metadata_not_found" => {
                            println!();
                            return Err(Error::Custom(format!("Could not find {hash}")));
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
                            if started {
                                println!();
                            }
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

    async fn put(&self, path: String, ex: ExecutorPtr) -> Result<()> {
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
                        error!("{e}");
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

        let rpc_client_putter = RpcClient::new(self.endpoint.clone(), ex.clone()).await?;
        let req = JsonRequest::new("put", JsonValue::Array(vec![JsonValue::String(path)]));
        let rep = rpc_client_putter.request(req).await?;
        let path_str = rep.get::<String>().unwrap().clone();

        loop {
            match subscription.receive().await {
                JsonResult::Notification(n) => {
                    let params = n.params.get::<HashMap<String, JsonValue>>().unwrap();
                    let info =
                        params.get("info").unwrap().get::<HashMap<String, JsonValue>>().unwrap();
                    let path = match info.get("path") {
                        Some(path) => path.get::<String>().unwrap(),
                        None => continue,
                    };
                    if *path != path_str {
                        continue;
                    }

                    match params.get("event").unwrap().get::<String>().unwrap().as_str() {
                        "insert_completed" => {
                            let id = info.get("hash").unwrap().get::<String>().unwrap().to_string();
                            println!("{id}");
                            break Ok(())
                        }
                        "insert_error" => {
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

    async fn list_resources(&self) -> Result<()> {
        let req = JsonRequest::new("list_resources", JsonValue::Array(vec![]));
        let rep = self.rpc_client.request(req).await?;

        let resources_json: Vec<JsonValue> = rep.clone().try_into().unwrap();
        let resources: Vec<Resource> = resources_json.into_iter().map(|v| v.into()).collect();

        for resource in resources.iter() {
            let tree: Vec<TreeNode<&str>> = vec![
                TreeNode::kv("ID", hash_to_string(&resource.hash)),
                TreeNode::kvc(
                    "Type",
                    resource.rtype.as_str().to_string(),
                    type_to_colorspec(&resource.rtype),
                ),
                TreeNode::kvc(
                    "Status",
                    resource.status.as_str().to_string(),
                    status_to_colorspec(&resource.status),
                ),
                TreeNode::kv("Chunks", {
                    if let ResourceType::Directory = resource.rtype {
                        format!(
                            "{}/{} ({}/{})",
                            resource.total_chunks_downloaded,
                            optional_value!(resource.total_chunks_count),
                            resource.target_chunks_downloaded,
                            optional_value!(resource.target_chunks_count)
                        )
                    } else {
                        format!(
                            "{}/{}",
                            resource.total_chunks_downloaded,
                            optional_value!(resource.total_chunks_count)
                        )
                    }
                }),
                TreeNode::kv("Bytes", {
                    if let ResourceType::Directory = resource.rtype {
                        format!(
                            "{} ({})",
                            optional_value!(resource.total_bytes_size, |x: u64| {
                                format_progress_bytes(resource.total_bytes_downloaded, x)
                            }),
                            optional_value!(resource.target_bytes_size, |x: u64| {
                                format_progress_bytes(resource.target_bytes_downloaded, x)
                            })
                        )
                    } else {
                        optional_value!(resource.total_bytes_size, |x: u64| format_progress_bytes(
                            resource.total_bytes_downloaded,
                            x
                        ))
                    }
                }),
            ];
            print_tree(&resource.path.to_string_lossy(), &tree);
        }

        Ok(())
    }

    async fn buckets(&self) -> Result<()> {
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

            let tree: Vec<TreeNode<String>> = nodes
                .into_iter()
                .map(|n| {
                    let node: Vec<JsonValue> = n.try_into().unwrap();
                    let node_id: JsonValue = node[0].clone();
                    let addresses: Vec<JsonValue> = node[1].clone().try_into().unwrap();

                    let addresses_vec: Vec<String> = addresses
                        .into_iter()
                        .map(|addr| TryInto::<String>::try_into(addr).unwrap())
                        .collect();

                    let node_id_string: String = node_id.try_into().unwrap();

                    TreeNode {
                        key: node_id_string,
                        value: None,
                        color: None,
                        children: addresses_vec
                            .into_iter()
                            .map(|addr| TreeNode::key(addr.clone()))
                            .collect(),
                    }
                })
                .collect();

            print_tree(format!("Bucket {bucket_i}").as_str(), &tree);
        }

        if empty {
            println!("All buckets are empty");
        }

        Ok(())
    }

    async fn seeders(&self) -> Result<()> {
        let req = JsonRequest::new("list_seeders", JsonValue::Array(vec![]));
        let rep = self.rpc_client.request(req).await?;

        let resources: HashMap<String, JsonValue> = rep["seeders"].clone().try_into().unwrap();

        if resources.is_empty() {
            println!("No known seeders");
            return Ok(())
        }

        for (hash, nodes) in resources {
            let nodes: Vec<JsonValue> = nodes.try_into().unwrap();
            let tree: Vec<TreeNode<String>> = nodes
                .into_iter()
                .map(|n| {
                    let node: Vec<JsonValue> = n.try_into().unwrap();
                    let node_id: JsonValue = node[0].clone();
                    let addresses: Vec<JsonValue> = node[1].clone().try_into().unwrap();

                    let addresses_vec: Vec<String> = addresses
                        .into_iter()
                        .map(|addr| TryInto::<String>::try_into(addr).unwrap())
                        .collect();

                    let node_id_string: String = node_id.try_into().unwrap();

                    TreeNode {
                        key: node_id_string,
                        value: None,
                        color: None,
                        children: addresses_vec
                            .into_iter()
                            .map(|addr| TreeNode::key(addr.clone()))
                            .collect(),
                    }
                })
                .collect();

            print_tree(&hash, &tree);
        }

        Ok(())
    }

    async fn watch(&self, ex: ExecutorPtr) -> Result<()> {
        let req = JsonRequest::new("list_resources", JsonValue::Array(vec![]));
        let rep = self.rpc_client.request(req).await?;

        let resources_json: Vec<JsonValue> = rep.clone().try_into().unwrap();
        let resources: Arc<RwLock<Vec<Resource>>> = Arc::new(RwLock::new(vec![]));

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
                        error!("{e}");
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

        let mut update_resource = async |resource: &Resource| {
            let mut resources_write = resources.write().await;
            let i = match resources_write.iter().position(|r| r.hash == resource.hash) {
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

            // Hash
            print!("\r{:>44} ", hash_to_string(&resource.hash));

            // Type
            tstdout.set_color(&type_to_colorspec(&resource.rtype)).unwrap();
            print!(
                "{:>4} ",
                match resource.rtype.as_str() {
                    "unknown" => "?",
                    "directory" => "dir",
                    _ => resource.rtype.as_str(),
                }
            );
            tstdout.reset().unwrap();

            // Status
            tstdout.set_color(&status_to_colorspec(&resource.status)).unwrap();
            print!("{:>11} ", resource.status.as_str());
            tstdout.reset().unwrap();

            // Downloaded / Total (in bytes)
            match resource.total_bytes_size {
                0 => {
                    print!("{:>5.1} {:>16} ", 0.0, "?");
                }
                _ => {
                    let percent = resource.total_bytes_downloaded as f64 /
                        resource.total_bytes_size as f64 *
                        100.0;
                    if resource.total_bytes_downloaded == resource.total_bytes_size {
                        print!("{:>5.1} {:>16} ", percent, format_bytes(resource.total_bytes_size));
                    } else {
                        print!(
                            "{:>5.1} {:>16} ",
                            percent,
                            format_progress_bytes(
                                resource.total_bytes_downloaded,
                                resource.total_bytes_size
                            )
                        );
                    }
                }
            };

            // Downloaded / Total (in chunks)
            match resource.total_chunks_count {
                0 => {
                    print!("{:>9} ", format!("{}/?", resource.total_chunks_downloaded));
                }
                _ => {
                    if resource.total_chunks_downloaded == resource.total_chunks_count {
                        print!("{:>9} ", resource.total_chunks_count.to_string());
                    } else {
                        print!(
                            "{:>9} ",
                            format!(
                                "{}/{}",
                                resource.total_chunks_downloaded, resource.total_chunks_count
                            )
                        );
                    }
                }
            };

            // Download speed (in bytes/sec)
            let speed_available = resource.total_bytes_downloaded < resource.total_bytes_size &&
                resource.status.as_str() == "downloading" &&
                !resource.speeds.is_empty();
            print!(
                "{:>12} ",
                match speed_available {
                    false => "-".to_string(),
                    true => format!("{}/s", format_bytes(*resource.speeds.last().unwrap() as u64)),
                }
            );

            // ETA
            let eta = resource.get_eta();
            print!(
                "{:>6}",
                match eta {
                    0 => "-".to_string(),
                    _ => format_duration(eta),
                }
            );

            println!();

            // Move the cursor to end
            print!("\x1b[{};1H", resources_write.len() + 2);
            stdout().flush().unwrap();
        };

        let print_begin = async || {
            // Clear
            print!("\x1B[2J\x1B[1;1H");

            // Print column headers
            println!(
                "\x1b[4m{:>44} {:>4} {:>11} {:>5} {:>16} {:>9} {:>12} {:>6}\x1b[0m",
                "Hash", "Type", "Status", "%", "Bytes", "Chunks", "Speed", "ETA"
            );
        };

        print_begin().await;
        if resources_json.is_empty() {
            println!("No known resources");
        } else {
            for resource in resources_json.iter() {
                let rs: Resource = resource.clone().into();
                update_resource(&rs).await;
            }
        }

        loop {
            match subscription.receive().await {
                JsonResult::Notification(n) => {
                    let params = n.params.get::<HashMap<String, JsonValue>>().unwrap();
                    let info = params.get("info");
                    if info.is_none() {
                        continue
                    }
                    let info = info.unwrap().get::<HashMap<String, JsonValue>>().unwrap();
                    match params.get("event").unwrap().get::<String>().unwrap().as_str() {
                        "download_started" |
                        "metadata_download_completed" |
                        "chunk_download_completed" |
                        "download_completed" |
                        "missing_chunks" |
                        "metadata_not_found" |
                        "resource_updated" => {
                            let resource: Resource = info.get("resource").unwrap().clone().into();
                            update_resource(&resource).await;
                        }
                        "resource_removed" => {
                            {
                                let hash = info.get("hash").unwrap().get::<String>().unwrap();
                                let mut resources_write = resources.write().await;
                                let i = resources_write
                                    .iter()
                                    .position(|r| hash_to_string(&r.hash) == *hash);
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

    async fn verify(&self, files: Option<Vec<String>>) -> Result<()> {
        let files = files.unwrap_or_default().into_iter().map(JsonValue::String).collect();
        let req = JsonRequest::new("verify", JsonValue::Array(files));
        self.rpc_client.request(req).await?;
        Ok(())
    }

    async fn lookup(&self, hash: String, ex: ExecutorPtr) -> Result<()> {
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
                        error!("{e}");
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
        let req =
            JsonRequest::new("lookup", JsonValue::Array(vec![JsonValue::String(hash.clone())]));
        let rpc_client_lookup = RpcClient::new(self.endpoint.clone(), ex.clone()).await?;
        rpc_client_lookup.request(req).await?;

        let print_seeders = |info: &HashMap<String, JsonValue>| {
            let seeders = info.get("seeders").unwrap().get::<Vec<JsonValue>>().unwrap();
            for seeder in seeders {
                let seeder = seeder.get::<HashMap<String, JsonValue>>().unwrap();
                let node: HashMap<String, JsonValue> =
                    seeder.get("node").unwrap().clone().try_into().unwrap();
                let node_id: String = node.get("id").unwrap().clone().try_into().unwrap();
                let addresses: Vec<JsonValue> =
                    node.get("addresses").unwrap().clone().try_into().unwrap();
                let tree: Vec<_> = addresses
                    .into_iter()
                    .map(|addr| TreeNode::key(TryInto::<String>::try_into(addr).unwrap()))
                    .collect();

                print_tree(node_id.as_str(), &tree);
            }
        };

        loop {
            match subscription.receive().await {
                JsonResult::Notification(n) => {
                    let params = n.params.get::<HashMap<String, JsonValue>>().unwrap();
                    let info =
                        params.get("info").unwrap().get::<HashMap<String, JsonValue>>().unwrap();
                    let hash_ = match info.get("hash") {
                        Some(hash_value) => hash_value.get::<String>().unwrap(),
                        None => continue,
                    };
                    if hash != *hash_ {
                        continue;
                    }

                    if params.get("event").unwrap().get::<String>().unwrap().as_str() ==
                        "seeders_found"
                    {
                        print_seeders(info);
                        break
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
        Ok(())
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    setup_logging(args.verbose, None)?;

    let ex = Arc::new(smol::Executor::new());
    smol::block_on(async {
        ex.run(async {
            let rpc_client = Arc::new(RpcClient::new(args.endpoint.clone(), ex.clone()).await?);
            let fu = Fu { rpc_client, endpoint: args.endpoint.clone() };

            match args.command {
                Subcmd::Get { hash, path, files } => fu.get(hash, path, files, ex.clone()).await,
                Subcmd::Put { path } => fu.put(path, ex.clone()).await,
                Subcmd::Ls {} => fu.list_resources().await,
                Subcmd::Watch {} => fu.watch(ex.clone()).await,
                Subcmd::Rm { hash } => fu.remove(hash).await,
                Subcmd::Buckets {} => fu.buckets().await,
                Subcmd::Seeders {} => fu.seeders().await,
                Subcmd::Verify { files } => fu.verify(files).await,
                Subcmd::Lookup { hash } => fu.lookup(hash, ex.clone()).await,
            }?;

            Ok(())
        })
        .await
    })
}
