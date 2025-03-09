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

use std::{
    collections::{HashMap, HashSet},
    io::ErrorKind,
    path::PathBuf,
    sync::Arc,
};

use num_bigint::BigUint;

use async_trait::async_trait;
use dht::{Dht, DhtHandler, DhtNode, DhtRouter};
use futures::{
    future::{try_select, Either, FutureExt},
    pin_mut,
};
use log::{debug, error, info, warn};
use rand::{rngs::OsRng, RngCore};
use smol::{
    channel,
    fs::{File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
    lock::{Mutex, MutexGuard, RwLock},
    stream::StreamExt,
    Executor,
};
use structopt_toml::{structopt::StructOpt, StructOptToml};
use tinyjson::JsonValue;

use darkfi::{
    async_daemonize, cli_desc,
    geode::Geode,
    net::{session::SESSION_DEFAULT, settings::SettingsOpt, ChannelPtr, P2p, P2pPtr},
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult, JsonSubscriber},
        p2p_method::HandlerP2p,
        server::{listen_and_serve, RequestHandler},
        settings::{RpcSettings, RpcSettingsOpt},
    },
    system::{StoppableTask, StoppableTaskPtr},
    util::path::expand_path,
    Error, Result,
};

/// P2P protocols
mod proto;
use proto::{
    FudAnnounce, FudChunkReply, FudFileReply, FudFindNodesReply, FudFindNodesRequest,
    FudFindRequest, FudFindSeedersReply, FudFindSeedersRequest, FudPingReply, FudPingRequest,
    ProtocolFud,
};

mod dht;

const CONFIG_FILE: &str = "fud_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../fud_config.toml");

const NODE_ID_PATH: &str = "node_id";

#[derive(Clone, Debug, serde::Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "fud", about = cli_desc!())]
struct Args {
    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long)]
    /// Set log file path to output daemon logs into
    log: Option<String>,

    #[structopt(long, default_value = "~/.local/share/darkfi/fud")]
    /// Base directory for filesystem storage
    base_dir: String,

    #[structopt(flatten)]
    /// Network settings
    net: SettingsOpt,

    #[structopt(flatten)]
    /// JSON-RPC settings
    rpc: RpcSettingsOpt,
}

pub struct Fud {
    /// Key -> Seeders
    seeders_router: DhtRouter,

    /// Pointer to the P2P network instance
    p2p: P2pPtr,

    /// The Geode instance
    geode: Geode,

    /// The DHT instance
    dht: Arc<Dht>,

    file_fetch_tx: channel::Sender<(blake3::Hash, Result<()>)>,
    file_fetch_rx: channel::Receiver<(blake3::Hash, Result<()>)>,
    file_fetch_end_tx: channel::Sender<(blake3::Hash, Result<()>)>,
    file_fetch_end_rx: channel::Receiver<(blake3::Hash, Result<()>)>,
    chunk_fetch_tx: channel::Sender<(blake3::Hash, Result<()>)>,
    chunk_fetch_rx: channel::Receiver<(blake3::Hash, Result<()>)>,
    chunk_fetch_end_tx: channel::Sender<(blake3::Hash, Result<()>)>,
    chunk_fetch_end_rx: channel::Receiver<(blake3::Hash, Result<()>)>,

    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,

    /// dnet JSON-RPC subscriber
    dnet_sub: JsonSubscriber,
}

#[async_trait]
impl RequestHandler<()> for Fud {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        return match req.method.as_str() {
            "ping" => self.pong(req.id, req.params).await,

            "put" => self.put(req.id, req.params).await,
            "get" => self.get(req.id, req.params).await,

            "dnet.switch" => self.dnet_switch(req.id, req.params).await,
            "dnet.subscribe_events" => self.dnet_subscribe_events(req.id, req.params).await,
            "p2p.get_info" => self.p2p_get_info(req.id, req.params).await,
            "list_buckets" => self.list_buckets(req.id, req.params).await,
            "list_seeders" => self.list_seeders(req.id, req.params).await,
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'_, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

impl Fud {
    // RPCAPI:
    // Put a file onto the network. Takes a local filesystem path as a parameter.
    // Returns the file hash that serves as a pointer to the uploaded file.
    //
    // --> {"jsonrpc": "2.0", "method": "put", "params": ["/foo.txt"], "id": 42}
    // <-- {"jsonrpc": "2.0", "result: "df4...3db7", "id": 42}
    async fn put(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let path = params[0].get::<String>().unwrap();
        let path = match expand_path(path.as_str()) {
            Ok(v) => v,
            Err(_) => return JsonError::new(ErrorCode::InvalidParams, None, id).into(),
        };

        // A valid path was passed. Let's see if we can read it, and if so,
        // add it to Geode.
        let fd = match File::open(&path).await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to open {:?}: {}", path, e);
                return JsonError::new(ErrorCode::InvalidParams, None, id).into()
            }
        };

        let (file_hash, chunk_hashes) = match self.geode.insert(fd).await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed inserting file {:?} to geode: {}", path, e);
                return JsonError::new(ErrorCode::InternalError, None, id).into()
            }
        };

        // Announce file
        let self_node = self.dht.node.clone();
        let fud_announce = FudAnnounce { key: file_hash, nodes: vec![self_node.clone()] };
        let _ = self.announce(&file_hash, &fud_announce, self.seeders_router.clone()).await;

        // Announce chunks
        for chunk_hash in chunk_hashes {
            let fud_announce = FudAnnounce { key: chunk_hash, nodes: vec![self_node.clone()] };
            let _ = self.announce(&chunk_hash, &fud_announce, self.seeders_router.clone()).await;
        }

        JsonResponse::new(JsonValue::String(file_hash.to_hex().to_string()), id).into()
    }

    // RPCAPI:
    // Fetch a file from the network. Takes a file hash as parameter.
    // Returns the path to the assembled file, if found/fetched.
    //
    // --> {"jsonrpc": "2.0", "method": "get", "params": ["1211...abfd"], "id": 42}
    // <-- {"jsonrpc": "2.0", "result: "~/.local/share/darkfi/fud/downloads/fab1...2314", "id": 42}
    async fn get(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 2 || !params[0].is_string() || !params[1].is_string() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let file_name: Option<String> = match params[1].get::<String>() {
            Some(name) => match name.is_empty() {
                true => None,
                false => Some(name.clone()),
            },
            None => None,
        };

        let self_node = self.dht.node.clone();

        let file_hash = match blake3::Hash::from_hex(params[0].get::<String>().unwrap()) {
            Ok(v) => v,
            Err(_) => return JsonError::new(ErrorCode::InvalidParams, None, id).into(),
        };

        let chunked_file = match self.geode.get(&file_hash).await {
            Ok(v) => v,
            Err(Error::GeodeNeedsGc) => todo!(),
            Err(Error::GeodeFileNotFound) => {
                info!("Requested file {} not found in Geode, triggering fetch", file_hash);
                self.file_fetch_tx.send((file_hash, Ok(()))).await.unwrap();
                info!("Waiting for background file fetch task...");
                let (i_file_hash, status) = self.file_fetch_end_rx.recv().await.unwrap();
                match status {
                    Ok(()) => self.geode.get(&i_file_hash).await.unwrap(),

                    Err(Error::GeodeFileRouteNotFound) => {
                        // TODO: Return FileNotFound error
                        return JsonError::new(ErrorCode::InternalError, None, id).into()
                    }

                    Err(e) => panic!("{}", e),
                }
            }

            Err(e) => panic!("{}", e),
        };

        if chunked_file.is_complete() {
            let fud_announce = FudAnnounce { key: file_hash, nodes: vec![self_node.clone()] };
            let _ = self.announce(&file_hash, &fud_announce, self.seeders_router.clone()).await;

            return match self.geode.assemble_file(&file_hash, &chunked_file, file_name).await {
                Ok(file_path) => JsonResponse::new(
                    JsonValue::String(file_path.to_string_lossy().to_string()),
                    id,
                )
                .into(),
                Err(_) => JsonError::new(ErrorCode::InternalError, None, id).into(),
            }
        }

        // Fetch any missing chunks
        let mut missing_chunks = vec![];
        for (chunk, path) in chunked_file.iter() {
            if path.is_none() {
                missing_chunks.push(*chunk);
            }
        }

        for chunk in missing_chunks {
            self.chunk_fetch_tx.send((chunk, Ok(()))).await.unwrap();
            let (i_chunk_hash, status) = self.chunk_fetch_end_rx.recv().await.unwrap();

            match status {
                Ok(()) => {
                    let fud_announce =
                        FudAnnounce { key: i_chunk_hash, nodes: vec![self_node.clone()] };
                    let _ = self
                        .announce(&i_chunk_hash, &fud_announce, self.seeders_router.clone())
                        .await;
                }
                Err(Error::GeodeChunkRouteNotFound) => continue,

                Err(e) => panic!("{}", e),
            };
        }

        let chunked_file = match self.geode.get(&file_hash).await {
            Ok(v) => v,
            Err(e) => panic!("{}", e),
        };

        if !chunked_file.is_complete() {
            todo!();
            // TODO: Return JsonError missing chunks
        }

        return match self.geode.assemble_file(&file_hash, &chunked_file, file_name).await {
            Ok(file_path) => {
                JsonResponse::new(JsonValue::String(file_path.to_string_lossy().to_string()), id)
                    .into()
            }
            Err(_) => JsonError::new(ErrorCode::InternalError, None, id).into(),
        }
    }

    // RPCAPI:
    // Activate or deactivate dnet in the P2P stack.
    // By sending `true`, dnet will be activated, and by sending `false` dnet
    // will be deactivated. Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "dnet_switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn dnet_switch(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_bool() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let switch = params[0].get::<bool>().unwrap();

        if *switch {
            self.p2p.dnet_enable();
        } else {
            self.p2p.dnet_disable();
        }

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Initializes a subscription to p2p dnet events.
    // Once a subscription is established, `fud` will send JSON-RPC notifications of
    // new network events to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "dnet.subscribe_events", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "dnet.subscribe_events", "params": [`event`]}
    pub async fn dnet_subscribe_events(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        self.dnet_sub.clone().into()
    }

    // RPCAPI:
    // Returns the current buckets
    //
    // --> {"jsonrpc": "2.0", "method": "list_buckets", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [[["abcdef", ["tcp://127.0.0.1:13337"]]]], "id": 1}
    pub async fn list_buckets(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }
        let mut buckets = vec![];
        for bucket in self.dht.buckets.read().await.iter() {
            let mut nodes = vec![];
            for node in bucket.nodes.clone() {
                let mut addresses = vec![];
                for addr in node.addresses {
                    addresses.push(JsonValue::String(addr.to_string()));
                }
                nodes.push(JsonValue::Array(vec![
                    JsonValue::String(node.id.to_hex().to_string()),
                    JsonValue::Array(addresses),
                ]));
            }
            buckets.push(JsonValue::Array(nodes));
        }

        JsonResponse::new(JsonValue::Array(buckets), id).into()
    }

    // RPCAPI:
    // Returns the content of the seeders router
    //
    // --> {"jsonrpc": "2.0", "method": "list_routes", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {"seeders": {"abcdef": ["ghijkl"]}}, "id": 1}
    pub async fn list_seeders(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }
        let mut seeders_router: HashMap<String, JsonValue> = HashMap::new();
        for (hash, nodes) in self.seeders_router.read().await.iter() {
            let mut node_ids = vec![];
            for node in nodes {
                node_ids.push(JsonValue::String(node.id.to_hex().to_string()));
            }
            seeders_router.insert(hash.to_hex().to_string(), JsonValue::Array(node_ids));
        }
        let mut res: HashMap<String, JsonValue> = HashMap::new();
        res.insert("seeders".to_string(), JsonValue::Object(seeders_router));

        JsonResponse::new(JsonValue::Object(res), id).into()
    }
}

impl HandlerP2p for Fud {
    fn p2p(&self) -> P2pPtr {
        self.p2p.clone()
    }
}

enum FetchReply {
    File(FudFileReply),
    Chunk(FudChunkReply),
}

/// Fetch a file or chunk from the network
/// 1. Lookup nodes close to the key
/// 2. Request seeders for the file/chunk from those nodes
/// 3. Request the file/chunk from the seeders
async fn fetch(fud: Arc<Fud>, key: blake3::Hash) -> Option<FetchReply> {
    let mut queried_seeders: HashSet<blake3::Hash> = HashSet::new();
    let closest_nodes = fud.lookup_nodes(&key).await; // 1
    let mut result: Option<FetchReply> = None;
    if closest_nodes.is_err() {
        return None
    }

    for node in closest_nodes.unwrap() {
        // 2. Request list of seeders
        let channel = match fud.get_channel(&node).await {
            Ok(channel) => channel,
            Err(e) => {
                warn!(target: "fud::fetch()", "Could not get a channel for node {}: {}", node.id, e);
                continue;
            }
        };
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<FudFindSeedersReply>().await;

        let msg_subscriber = match channel.subscribe_msg::<FudFindSeedersReply>().await {
            Ok(msg_subscriber) => msg_subscriber,
            Err(e) => {
                warn!(target: "fud::fetch()", "Error subscribing to msg: {}", e);
                continue;
            }
        };

        let _ = channel.send(&FudFindSeedersRequest { key }).await;

        let reply = match msg_subscriber.receive_with_timeout(fud.dht().timeout).await {
            Ok(reply) => reply,
            Err(e) => {
                warn!(target: "fud::fetch()", "Error waiting for reply: {}", e);
                continue;
            }
        };

        let mut seeders = reply.nodes.clone();
        info!(target: "fud::fetch()", "Found seeders for {}: {:?}", key, seeders);

        msg_subscriber.unsubscribe().await;

        // 3. Request the file/chunk from the seeders
        while let Some(seeder) = seeders.pop() {
            // Only query a seeder once
            if queried_seeders.iter().any(|s| *s == seeder.id) {
                continue;
            }
            queried_seeders.insert(seeder.id);

            if let Ok(channel) = fud.get_channel(&seeder).await {
                let msg_subsystem = channel.message_subsystem();
                msg_subsystem.add_dispatch::<FudChunkReply>().await;
                msg_subsystem.add_dispatch::<FudFileReply>().await;
                let msg_subscriber_chunk = channel.subscribe_msg::<FudChunkReply>().await.unwrap();
                let msg_subscriber_file = channel.subscribe_msg::<FudFileReply>().await.unwrap();

                let _ = channel.send(&FudFindRequest { key }).await;

                let chunk_recv =
                    msg_subscriber_chunk.receive_with_timeout(fud.dht().timeout).fuse();
                let file_recv = msg_subscriber_file.receive_with_timeout(fud.dht().timeout).fuse();

                pin_mut!(chunk_recv, file_recv);

                // Wait for a FudChunkReply or a FudFileReply
                match try_select(chunk_recv, file_recv).await {
                    Ok(Either::Left((chunk_reply, _))) => {
                        info!(target: "fud::fetch()", "Received chunk {} from seeder {:?}", key, seeder.id);
                        msg_subscriber.unsubscribe().await;
                        result = Some(FetchReply::Chunk((*chunk_reply).clone()));
                        break;
                    }
                    Ok(Either::Right((file_reply, _))) => {
                        info!(target: "fud::fetch()", "Received file {} from seeder {:?}", key, seeder.id);
                        msg_subscriber.unsubscribe().await;
                        result = Some(FetchReply::File((*file_reply).clone()));
                        break;
                    }
                    Err(e) => {
                        match e {
                            Either::Left((chunk_err, _)) => {
                                warn!(target: "fud::fetch()", "Error waiting for chunk reply: {}", chunk_err);
                            }
                            Either::Right((file_err, _)) => {
                                warn!(target: "fud::fetch()", "Error waiting for file reply: {}", file_err);
                            }
                        };
                        msg_subscriber.unsubscribe().await;
                        continue;
                    }
                };
            }
        }

        if result.is_some() {
            break;
        }
    }

    result
}

/// Background task that receives file fetch requests and tries to
/// fetch objects from the network using the routing table.
/// TODO: This can be optimised a lot for connection reuse, etc.
async fn fetch_file_task(fud: Arc<Fud>, _: Arc<Executor<'_>>) -> Result<()> {
    info!(target: "fud::fetch_file_task()", "Started background file fetch task");
    loop {
        let (file_hash, _) = fud.file_fetch_rx.recv().await.unwrap();
        info!(target: "fud::fetch_file_task()", "Fetching file {}", file_hash);

        let result = fetch(fud.clone(), file_hash).await;

        match result {
            Some(reply) => {
                match reply {
                    FetchReply::File(FudFileReply { chunk_hashes }) => {
                        if let Err(e) = fud.geode.insert_file(&file_hash, &chunk_hashes).await {
                            error!("Failed inserting file {} to Geode: {}", file_hash, e);
                        }
                        fud.file_fetch_end_tx.send((file_hash, Ok(()))).await.unwrap();
                    }
                    // Looked for a file but got a chunk, meaning that file_hash = chunk_hash, the file fits in a single chunk
                    FetchReply::Chunk(FudChunkReply { chunk }) => {
                        // TODO: Verify chunk
                        info!(target: "fud::fetch()", "File fits in a single chunk");
                        let _ = fud.geode.insert_file(&file_hash, &[file_hash]).await;
                        match fud.geode.insert_chunk(&chunk).await {
                            Ok(inserted_hash) => {
                                if inserted_hash != file_hash {
                                    warn!("Received chunk does not match requested file");
                                }
                            }
                            Err(e) => {
                                error!("Failed inserting chunk {} to Geode: {}", file_hash, e);
                            }
                        };
                        fud.file_fetch_end_tx.send((file_hash, Ok(()))).await.unwrap();
                    }
                }
            }
            None => {
                fud.file_fetch_end_tx
                    .send((file_hash, Err(Error::GeodeFileRouteNotFound)))
                    .await
                    .unwrap();
            }
        };
    }
}

/// Background task that receives chunk fetch requests and tries to
/// fetch objects from the network using the routing table.
/// TODO: This can be optimised a lot for connection reuse, etc.
async fn fetch_chunk_task(fud: Arc<Fud>, _: Arc<Executor<'_>>) -> Result<()> {
    info!(target: "fud::fetch_chunk_task()", "Started background chunk fetch task");
    loop {
        let (chunk_hash, _) = fud.chunk_fetch_rx.recv().await.unwrap();
        info!(target: "fud::fetch_chunk_task()", "Fetching chunk {}", chunk_hash);

        let result = fetch(fud.clone(), chunk_hash).await;

        match result {
            Some(reply) => {
                match reply {
                    FetchReply::Chunk(FudChunkReply { chunk }) => {
                        // TODO: Verify chunk
                        match fud.geode.insert_chunk(&chunk).await {
                            Ok(inserted_hash) => {
                                if inserted_hash != chunk_hash {
                                    warn!("Received chunk does not match requested chunk");
                                }
                            }
                            Err(e) => {
                                error!("Failed inserting chunk {} to Geode: {}", chunk_hash, e);
                            }
                        };
                        fud.chunk_fetch_end_tx.send((chunk_hash, Ok(()))).await.unwrap();
                    }
                    _ => {
                        // Looked for a chunk but got a file instead, not supposed to happen
                        fud.chunk_fetch_end_tx
                            .send((chunk_hash, Err(Error::GeodeChunkRouteNotFound)))
                            .await
                            .unwrap();
                    }
                }
            }
            None => {
                fud.chunk_fetch_end_tx
                    .send((chunk_hash, Err(Error::GeodeChunkRouteNotFound)))
                    .await
                    .unwrap();
            }
        };
    }
}

#[async_trait]
impl DhtHandler for Fud {
    fn dht(&self) -> Arc<Dht> {
        self.dht.clone()
    }

    async fn ping(&self, channel: ChannelPtr) -> Result<dht::DhtNode> {
        debug!(target: "fud::Fud::DhtHandler::ping()", "Sending ping to channel {}", channel.info.id);
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<FudPingReply>().await;
        let msg_subscriber = channel.subscribe_msg::<FudPingReply>().await.unwrap();
        let request = FudPingRequest {};

        channel.send(&request).await?;

        let reply = msg_subscriber.receive_with_timeout(self.dht().timeout).await?;

        msg_subscriber.unsubscribe().await;

        Ok(reply.node.clone())
    }

    // TODO: Optimize this
    async fn on_new_node(&self, node: &DhtNode) -> Result<()> {
        debug!(target: "fud::Fud::DhtHandler::on_new_node()", "New node {}", node.id);

        // If this is the first node we know about, then bootstrap
        if !self.dht().is_bootstrapped().await {
            self.dht().set_bootstrapped().await;

            // Lookup our own node id
            let self_node = self.dht().node.clone();
            debug!(target: "fud::Fud::DhtHandler::on_new_node()", "DHT bootstrapping {}", self_node.id);
            let _ = self.lookup_nodes(&self_node.id).await;
        }

        // Send keys that are closer to this node than we are
        let self_id = self.dht().node.id;
        let channel = self.get_channel(node).await?;
        for (key, nodes) in self.seeders_router.read().await.iter() {
            let node_distance = BigUint::from_bytes_be(&self.dht().distance(key, &node.id));
            let self_distance = BigUint::from_bytes_be(&self.dht().distance(key, &self_id));
            if node_distance <= self_distance {
                let _ = channel
                    .send(&FudAnnounce { key: *key, nodes: nodes.iter().cloned().collect() })
                    .await;
            }
        }

        Ok(())
    }

    async fn fetch_nodes(&self, node: &DhtNode, key: &blake3::Hash) -> Result<Vec<DhtNode>> {
        debug!(target: "fud::Fud::DhtHandler::fetch_value()", "Fetching nodes close to {} from node {}", key, node.id);

        let channel = self.get_channel(node).await?;
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<FudFindNodesReply>().await;
        let msg_subscriber_nodes = channel.subscribe_msg::<FudFindNodesReply>().await.unwrap();

        let request = FudFindNodesRequest { key: *key };
        channel.send(&request).await?;

        let reply = msg_subscriber_nodes.receive_with_timeout(self.dht().timeout).await?;

        msg_subscriber_nodes.unsubscribe().await;

        Ok(reply.nodes.clone())
    }
}

// TODO: This is not Sybil-resistant
fn generate_node_id() -> Result<blake3::Hash> {
    let mut rng = OsRng;
    let mut random_data = [0u8; 32];
    rng.fill_bytes(&mut random_data);
    let node_id = blake3::Hash::from_bytes(random_data);
    Ok(node_id)
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
    // The working directory for this daemon and geode.
    let basedir = expand_path(&args.base_dir)?;

    // Hashmap used for routing
    let seeders_router = Arc::new(RwLock::new(HashMap::new()));

    info!("Instantiating Geode instance");
    let geode = Geode::new(&basedir).await?;

    info!("Instantiating P2P network");
    let p2p = P2p::new(args.net.into(), ex.clone()).await?;

    let external_addrs = p2p.hosts().external_addrs().await;

    if external_addrs.is_empty() {
        error!(
            target: "fud::realmain",
            "External addrs not configured. Stopping",
        );
        return Ok(())
    }

    info!("Starting dnet subs task");
    let dnet_sub = JsonSubscriber::new("dnet.subscribe_events");
    let dnet_sub_ = dnet_sub.clone();
    let p2p_ = p2p.clone();
    let dnet_task = StoppableTask::new();
    dnet_task.clone().start(
        async move {
            let dnet_sub = p2p_.dnet_subscribe().await;
            loop {
                let event = dnet_sub.receive().await;
                debug!("Got dnet event: {:?}", event);
                dnet_sub_.notify(vec![event.into()].into()).await;
            }
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => panic!("{}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    // Get or generate the node id
    let node_id: Result<blake3::Hash> = {
        let mut node_id_path: PathBuf = basedir.clone();
        node_id_path.push(NODE_ID_PATH);
        match File::open(node_id_path.clone()).await {
            Ok(mut file) => {
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer).await?;
                let buf: [u8; 64] = buffer.try_into().expect("Node ID must have 64 characters");
                let node_id = blake3::Hash::from_hex(buf)?;
                Ok(node_id)
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                let node_id = generate_node_id()?;
                let mut file =
                    OpenOptions::new().write(true).create(true).open(node_id_path).await?;
                file.write_all(node_id.to_hex().as_bytes()).await?;
                file.flush().await?;
                Ok(node_id)
            }
            Err(e) => Err(e.into()),
        }
    };

    let node_id_ = node_id?;

    info!(target: "fud", "Your node ID: {}", node_id_);

    // Daemon instantiation
    let (file_fetch_tx, file_fetch_rx) = smol::channel::unbounded();
    let (file_fetch_end_tx, file_fetch_end_rx) = smol::channel::unbounded();
    let (chunk_fetch_tx, chunk_fetch_rx) = smol::channel::unbounded();
    let (chunk_fetch_end_tx, chunk_fetch_end_rx) = smol::channel::unbounded();
    // TODO: Add DHT settings in the config file
    let dht = Arc::new(Dht::new(&node_id_, 4, 16, 15, p2p.clone(), ex.clone()).await);
    let fud = Arc::new(Fud {
        seeders_router,
        p2p: p2p.clone(),
        geode,
        dht: dht.clone(),
        file_fetch_tx,
        file_fetch_rx,
        file_fetch_end_tx,
        file_fetch_end_rx,
        chunk_fetch_tx,
        chunk_fetch_rx,
        chunk_fetch_end_tx,
        chunk_fetch_end_rx,
        rpc_connections: Mutex::new(HashSet::new()),
        dnet_sub,
    });

    info!(target: "fud", "Starting fetch file task");
    let file_task = StoppableTask::new();
    file_task.clone().start(
        fetch_file_task(fud.clone(), ex.clone()),
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => error!(target: "fud", "Failed starting fetch file task: {}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    info!(target: "fud", "Starting fetch chunk task");
    let chunk_task = StoppableTask::new();
    chunk_task.clone().start(
        fetch_chunk_task(fud.clone(), ex.clone()),
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => error!(target: "fud", "Failed starting fetch chunk task: {}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    let rpc_settings: RpcSettings = args.rpc.into();
    info!(target: "fud", "Starting JSON-RPC server on {}", rpc_settings.listen);
    let rpc_task = StoppableTask::new();
    let fud_ = fud.clone();
    rpc_task.clone().start(
        listen_and_serve(rpc_settings, fud.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => fud_.stop_connections().await,
                Err(e) => error!(target: "fud", "Failed starting sync JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    info!("Starting P2P protocols");
    let registry = p2p.protocol_registry();
    let fud_ = fud.clone();
    registry
        .register(SESSION_DEFAULT, move |channel, p2p| {
            let fud_ = fud_.clone();
            async move { ProtocolFud::init(fud_, channel, p2p).await.unwrap() }
        })
        .await;
    p2p.clone().start().await?;

    info!(target: "fud", "Starting DHT tasks");
    let dht_channel_task = StoppableTask::new();
    let fud_ = fud.clone();
    dht_channel_task.clone().start(
        async move { fud_.channel_task::<FudFindNodesReply>().await },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => error!(target: "fud", "Failed starting dht channel task: {}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );
    let dht_disconnect_task = StoppableTask::new();
    let fud_ = fud.clone();
    dht_disconnect_task.clone().start(
        async move { fud_.disconnect_task().await },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => error!(target: "fud", "Failed starting dht disconnect task: {}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    info!(target: "fud", "Stopping fetch file task...");
    file_task.stop().await;

    info!(target: "fud", "Stopping fetch chunk task...");
    chunk_task.stop().await;

    info!(target: "fud", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    info!(target: "fud", "Stopping P2P network...");
    p2p.stop().await;

    info!(target: "fud", "Stopping DHT tasks");
    dht_channel_task.stop().await;
    dht_disconnect_task.stop().await;

    info!("Bye!");
    Ok(())
}
