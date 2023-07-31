/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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
    ffi::OsString,
};

use async_std::{
    channel,
    fs::File,
    stream::StreamExt,
    sync::{Arc, RwLock},
};
use async_trait::async_trait;
use log::{debug, error, info, warn};
use serde_json::{json, Value};
use smol::Executor;
use structopt_toml::{structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize, cli_desc,
    geode::Geode,
    net::{
        self, connector::Connector, protocol::ProtocolVersion, session::Session,
        settings::SettingsOpt, P2p, P2pPtr,
    },
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::{listen_and_serve, RequestHandler},
    },
    util::path::expand_path,
    Error, Result,
};

/// P2P protocols
mod proto;
use proto::{
    FudChunkReply, FudChunkRequest, FudFilePut, FudFileReply, FudFileRequest, ProtocolFud,
};

const CONFIG_FILE: &str = "fud_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../fud_config.toml");

#[derive(Clone, Debug, serde::Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "fud", about = cli_desc!())]
struct Args {
    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[structopt(long, default_value = "tcp://127.0.0.1:13336")]
    /// JSON-RPC listen URL
    rpc_listen: Url,

    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long)]
    /// Set log file path to output daemon logs into
    log: Option<String>,

    #[structopt(long, default_value = "~/.local/share/fud")]
    /// Base directory for filesystem storage
    base_dir: String,

    #[structopt(flatten)]
    /// Network settings
    net: SettingsOpt,
}

pub struct Fud {
    /// Routing table for file metadata
    metadata_router: Arc<RwLock<HashMap<blake3::Hash, HashSet<Url>>>>,
    /// Routing table for file chunks
    chunks_router: Arc<RwLock<HashMap<blake3::Hash, HashSet<Url>>>>,
    /// Pointer to the P2P network instance
    p2p: P2pPtr,
    /// The Geode instance
    geode: Geode,

    file_fetch_tx: channel::Sender<(blake3::Hash, Result<()>)>,
    file_fetch_rx: channel::Receiver<(blake3::Hash, Result<()>)>,
    chunk_fetch_tx: channel::Sender<(blake3::Hash, Result<()>)>,
    chunk_fetch_rx: channel::Receiver<(blake3::Hash, Result<()>)>,
}

#[async_trait]
impl RequestHandler for Fud {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return JsonError::new(ErrorCode::InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        match req.method.as_str() {
            Some("put") => return self.put(req.id, params).await,
            Some("get") => return self.get(req.id, params).await,

            Some("ping") => return self.pong(req.id, params).await,
            Some("dnet_switch") => return self.dnet_switch(req.id, params).await,
            Some("dnet_info") => return self.dnet_info(req.id, params).await,
            Some(_) | None => return JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }
}

impl Fud {
    // RPCAPI:
    // Put a file onto the network. Takes a local filesystem path as a parameter.
    // Returns the file hash that serves as a pointer to the uploaded file.
    //
    // --> {"jsonrpc": "2.0", "method": "put", "params": ["/foo.txt"], "id": 42}
    // <-- {"jsonrpc": "2.0", "result: "df4...3db7", "id": 42}
    async fn put(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let path = match expand_path(params[0].as_str().unwrap()) {
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

        let fud_file = FudFilePut { file_hash, chunk_hashes };
        self.p2p.broadcast(&fud_file).await;

        JsonResponse::new(json!(file_hash.to_hex().as_str()), id).into()
    }

    // RPCAPI:
    // Fetch a file from the network. Takes a file hash as parameter.
    // Returns the paths to the local chunks of the file, if found/fetched.
    //
    // --> {"jsonrpc": "2.0", "method": "get", "params": ["1211...abfd"], "id": 42}
    // <-- {"jsonrpc": "2.0", "result: ["~/.local/share/fud/chunks/fab1...2314", ...], "id": 42}
    async fn get(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let file_hash = match blake3::Hash::from_hex(params[0].as_str().unwrap()) {
            Ok(v) => v,
            Err(_) => return JsonError::new(ErrorCode::InvalidParams, None, id).into(),
        };

        let chunked_file = match self.geode.get(&file_hash).await {
            Ok(v) => v,
            Err(e) => todo!(), // fetch file
        };

        if chunked_file.is_complete() {
            let chunks: Vec<OsString> = chunked_file
                .iter()
                .map(|(_, path)| path.as_ref().unwrap().clone().into_os_string())
                .collect();

            return JsonResponse::new(json!(chunks), id).into()
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
            let (_, result) = self.chunk_fetch_rx.recv().await.unwrap();
            match result {
                Ok(()) => continue,
                Err(e) => todo!(),
            }
        }

        let chunked_file = match self.geode.get(&file_hash).await {
            Ok(v) => v,
            Err(e) => todo!(),
        };

        if !chunked_file.is_complete() {
            todo!();
            // Return JsonError missing chunks
        }

        let chunks: Vec<OsString> = chunked_file
            .iter()
            .map(|(_, path)| path.as_ref().unwrap().clone().into_os_string())
            .collect();

        JsonResponse::new(json!(chunks), id).into()
    }

    // RPCAPI:
    // Replies to a ping method.
    //
    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 42}
    async fn pong(&self, id: Value, _params: &[Value]) -> JsonResult {
        JsonResponse::new(json!("pong"), id).into()
    }

    // RPCAPI:
    // Activate or deactivate dnet in the P2P stack.
    // By sending `true`, dnet will be activated, and by sending `false` dnet
    // will be deactivated. Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "dnet_switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn dnet_switch(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 && params[0].as_bool().is_none() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        if params[0].as_bool().unwrap() {
            self.p2p.dnet_enable().await;
        } else {
            self.p2p.dnet_disable().await;
        }

        JsonResponse::new(json!(true), id).into()
    }

    // RPCAPI:
    // Retrieves P2P network information.
    // --> {"jsonrpc": "2.0", "method": "dnet_info", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", result": {"nodeID": [], "nodeinfo": [], "id": 42}
    async fn dnet_info(&self, id: Value, _params: &[Value]) -> JsonResult {
        let dnet_info = self.p2p.dnet_info().await;
        JsonResponse::new(P2p::map_dnet_info(dnet_info), id).into()
    }
}

/// Background task that receives file fetch requests and tries to
/// fetch objects from the network using the routing table.
/// TODO: This can be optimised a lot for connection reuse, etc.
async fn fetch_file_task(fud: Arc<Fud>, executor: Arc<Executor<'_>>) {
    info!("Started background file fetch task");
    loop {
        let (file_hash, _) = fud.file_fetch_rx.recv().await.unwrap();
        info!("fetch_file_task: Received {}", file_hash);

        let mut metadata_router = fud.metadata_router.write().await;
        let peers = metadata_router.get_mut(&file_hash);

        if peers.is_none() {
            warn!("File {} not in routing table, cannot fetch", file_hash);
            fud.file_fetch_tx.send((file_hash, Err(Error::GeodeFileRouteNotFound))).await.unwrap();
            continue
        }

        let mut found = false;
        let peers = peers.unwrap();
        let mut invalid_file_routes = vec![];

        for peer in peers.iter() {
            let session_out = fud.p2p.session_outbound().await;
            let session_weak = Arc::downgrade(&fud.p2p.session_outbound().await);

            info!("Connecting to {} to fetch {}", peer, file_hash);
            let connector = Connector::new(fud.p2p.settings(), Arc::new(session_weak));
            match connector.connect(peer).await {
                Ok((url, channel)) => {
                    let proto_ver = ProtocolVersion::new(
                        channel.clone(),
                        fud.p2p.settings().clone(),
                        fud.p2p.hosts().clone(),
                    )
                    .await;

                    let handshake_task = session_out.perform_handshake_protocols(
                        proto_ver,
                        channel.clone(),
                        executor.clone(),
                    );

                    channel.clone().start(executor.clone());

                    if let Err(e) = handshake_task.await {
                        error!("Handshake with {} failed: {}", url, e);
                        // Delete peer from router
                        invalid_file_routes.push(peer.clone());
                        continue
                    }

                    let msg_subscriber = channel.subscribe_msg::<FudFileReply>().await.unwrap();
                    let request = FudFileRequest { file_hash };

                    if let Err(e) = channel.send(&request).await {
                        error!("Failed sending FudFileRequest({}) to {}: {}", file_hash, url, e);
                        continue
                    }

                    // TODO: With timeout!
                    let reply = match msg_subscriber.receive().await {
                        Ok(v) => v,
                        Err(e) => {
                            error!("Error receiving FudFileReply from subscriber: {}", e);
                            continue
                        }
                    };

                    msg_subscriber.unsubscribe().await;
                    channel.stop().await;

                    if let Err(e) = fud.geode.insert_file(&file_hash, &reply.chunk_hashes).await {
                        error!("Failed inserting file {} to Geode: {}", file_hash, e);
                        continue
                    }

                    found = true;
                    break
                }

                Err(e) => {
                    error!("Failed to connect to {}: {}", peer, e);
                    continue
                }
            }
        }

        for peer in invalid_file_routes {
            debug!("Removing peer {} from {} file router", peer, file_hash);
            peers.remove(&peer);
        }

        if !found {
            warn!("Did not manage to fetch {} file metadata", file_hash);
            fud.file_fetch_tx.send((file_hash, Err(Error::GeodeFileRouteNotFound))).await.unwrap();
            continue
        }

        info!("Successfully fetched {} file metadata", file_hash);
        fud.file_fetch_tx.send((file_hash, Ok(()))).await.unwrap();
    }
}

/// Background task that receives chunk fetch requests and tries to
/// fetch objects from the network using the routing table.
/// TODO: This can be optimised a lot for connection reuse, etc.
async fn fetch_chunk_task(fud: Arc<Fud>, executor: Arc<Executor<'_>>) {
    info!("Started background chunk fetch task");
    loop {
        let (chunk_hash, _) = fud.chunk_fetch_rx.recv().await.unwrap();
        info!("fetch_chunk_task: Received {}", chunk_hash);

        let mut chunk_router = fud.chunks_router.write().await;
        let peers = chunk_router.get_mut(&chunk_hash);

        if peers.is_none() {
            warn!("Chunk {} not in routing table, cannot fetch", chunk_hash);
            fud.chunk_fetch_tx
                .send((chunk_hash, Err(Error::GeodeChunkRouteNotFound)))
                .await
                .unwrap();
            continue
        }

        let mut found = false;
        let peers = peers.unwrap();
        let mut invalid_chunk_routes = vec![];

        for peer in peers.iter() {
            let session_out = fud.p2p.session_outbound().await;
            let session_weak = Arc::downgrade(&fud.p2p.session_outbound().await);

            info!("Connecting to {} to fetch {}", peer, chunk_hash);
            let connector = Connector::new(fud.p2p.settings(), Arc::new(session_weak));
            match connector.connect(peer).await {
                Ok((url, channel)) => {
                    let proto_ver = ProtocolVersion::new(
                        channel.clone(),
                        fud.p2p.settings().clone(),
                        fud.p2p.hosts().clone(),
                    )
                    .await;

                    let handshake_task = session_out.perform_handshake_protocols(
                        proto_ver,
                        channel.clone(),
                        executor.clone(),
                    );

                    channel.clone().start(executor.clone());

                    if let Err(e) = handshake_task.await {
                        error!("Handshake with {} failed: {}", url, e);
                        // Delete peer from router
                        invalid_chunk_routes.push(peer.clone());
                        continue
                    }

                    let msg_subscriber = channel.subscribe_msg::<FudChunkReply>().await.unwrap();
                    let request = FudChunkRequest { chunk_hash };

                    if let Err(e) = channel.send(&request).await {
                        error!("Failed sending FudChunkRequest({}) to {}: {}", chunk_hash, url, e);
                        continue
                    }

                    // TODO: With timeout!
                    let reply = match msg_subscriber.receive().await {
                        Ok(v) => v,
                        Err(e) => {
                            error!("Error receiving FudChunkReply from subscriber: {}", e);
                            continue
                        }
                    };

                    msg_subscriber.unsubscribe().await;
                    channel.stop().await;

                    match fud.geode.insert_chunk(&reply.chunk).await {
                        Ok(inserted_hash) => {
                            if inserted_hash != chunk_hash {
                                warn!("Received chunk does not match requested chunk");
                                invalid_chunk_routes.push(peer.clone());
                                continue
                            }
                        }
                        Err(e) => {
                            error!("Failed inserting chunk {} to Geode: {}", chunk_hash, e);
                            continue
                        }
                    }

                    found = true;
                    break
                }

                Err(e) => {
                    error!("Failed to connect to {}: {}", peer, e);
                    continue
                }
            }
        }

        for peer in invalid_chunk_routes {
            debug!("Removing peer {} from {} chunk router", peer, chunk_hash);
            peers.remove(&peer);
        }

        if !found {
            warn!("Did not manage to fetch {} chunk", chunk_hash);
            fud.chunk_fetch_tx.send((chunk_hash, Err(Error::GeodeChunkNotFound))).await.unwrap();
            continue
        }

        info!("Successfully fetched {} chunk", chunk_hash);
        fud.chunk_fetch_tx.send((chunk_hash, Ok(()))).await.unwrap();
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'_>>) -> Result<()> {
    // The working directory for this daemon and geode.
    let basedir = expand_path(&args.base_dir)?;

    // Hashmaps used for routing
    let metadata_router = Arc::new(RwLock::new(HashMap::new()));
    let chunks_router = Arc::new(RwLock::new(HashMap::new()));

    info!("Instantiating Geode instance");
    let geode = Geode::new(&basedir.into()).await?;

    info!("Instantiating P2P network");
    let p2p = P2p::new(args.net.into()).await;

    // Daemon instantiation
    let (file_fetch_tx, file_fetch_rx) = smol::channel::unbounded();
    let (chunk_fetch_tx, chunk_fetch_rx) = smol::channel::unbounded();
    let fud = Arc::new(Fud {
        metadata_router,
        chunks_router,
        p2p: p2p.clone(),
        geode,
        file_fetch_tx,
        file_fetch_rx,
        chunk_fetch_tx,
        chunk_fetch_rx,
    });
    let _fud = fud.clone();
    ex.spawn(fetch_file_task(fud.clone(), ex.clone())).detach();
    ex.spawn(fetch_chunk_task(fud.clone(), ex.clone())).detach();

    info!("Starting JSON-RPC server on {}", args.rpc_listen);
    let _ex = ex.clone();
    ex.spawn(listen_and_serve(args.rpc_listen, fud, _ex)).detach();

    info!("Starting P2P protocols");
    let registry = p2p.protocol_registry();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let _fud = _fud.clone();
            async move { ProtocolFud::init(_fud, channel, p2p).await.unwrap() }
        })
        .await;
    p2p.clone().start(ex.clone()).await?;

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new()?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    info!("Stopping P2P network");
    p2p.stop().await;

    info!("Bye!");
    Ok(())
}
