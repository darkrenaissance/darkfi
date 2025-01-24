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
    sync::Arc,
};

use async_trait::async_trait;
use log::{debug, error, info, warn};
use smol::{
    channel,
    fs::File,
    lock::{Mutex, MutexGuard, RwLock},
    stream::StreamExt,
    Executor,
};
use structopt_toml::{structopt::StructOpt, StructOptToml};
use tinyjson::JsonValue;
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
    system::{StoppableTask, StoppableTaskPtr},
    util::path::expand_path,
    Error, Result,
};

/// P2P protocols
mod proto;
use proto::{
    FudChunkPut, FudChunkReply, FudChunkRequest, FudFilePut, FudFileReply, FudFileRequest,
    ProtocolFud,
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

    #[structopt(long, default_value = "~/.local/share/darkfi/fud")]
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

    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

#[async_trait]
impl RequestHandler<()> for Fud {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        return match req.method.as_str() {
            "ping" => self.pong(req.id, req.params).await,

            "put" => self.put(req.id, req.params).await,
            "get" => self.get(req.id, req.params).await,

            "dnet_switch" => self.dnet_switch(req.id, req.params).await,
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

        let fud_file = FudFilePut { file_hash, chunk_hashes };
        self.p2p.broadcast(&fud_file).await;

        JsonResponse::new(JsonValue::String(file_hash.to_hex().to_string()), id).into()
    }

    // RPCAPI:
    // Fetch a file from the network. Takes a file hash as parameter.
    // Returns the paths to the local chunks of the file, if found/fetched.
    //
    // --> {"jsonrpc": "2.0", "method": "get", "params": ["1211...abfd"], "id": 42}
    // <-- {"jsonrpc": "2.0", "result: ["~/.local/share/darkfi/fud/chunks/fab1...2314", ...], "id": 42}
    async fn get(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

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
                let (i_file_hash, status) = self.file_fetch_rx.recv().await.unwrap();
                match status {
                    Ok(()) => {
                        let ch_file = self.geode.get(&file_hash).await.unwrap();

                        let m = FudFilePut {
                            file_hash: i_file_hash,
                            chunk_hashes: ch_file.iter().map(|(h, _)| *h).collect(),
                        };

                        self.p2p.broadcast(&m).await;

                        ch_file
                    }

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
            let chunks: Vec<JsonValue> = chunked_file
                .iter()
                .map(|(_, path)| {
                    JsonValue::String(
                        path.as_ref().unwrap().clone().into_os_string().into_string().unwrap(),
                    )
                })
                .collect();

            return JsonResponse::new(JsonValue::Array(chunks), id).into()
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
            let (i_chunk_hash, status) = self.chunk_fetch_rx.recv().await.unwrap();

            match status {
                Ok(()) => {
                    let m = FudChunkPut { chunk_hash: i_chunk_hash };
                    self.p2p.broadcast(&m).await;
                    break
                }
                Err(Error::GeodeChunkRouteNotFound) => continue,

                Err(e) => panic!("{}", e),
            }
        }

        let chunked_file = match self.geode.get(&file_hash).await {
            Ok(v) => v,
            Err(e) => panic!("{}", e),
        };

        if !chunked_file.is_complete() {
            todo!();
            // Return JsonError missing chunks
        }

        let chunks: Vec<JsonValue> = chunked_file
            .iter()
            .map(|(_, path)| {
                JsonValue::String(
                    path.as_ref().unwrap().clone().into_os_string().into_string().unwrap(),
                )
            })
            .collect();

        JsonResponse::new(JsonValue::Array(chunks), id).into()
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
            self.p2p.dnet_enable().await;
        } else {
            self.p2p.dnet_disable().await;
        }

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }
}

/// Background task that receives file fetch requests and tries to
/// fetch objects from the network using the routing table.
/// TODO: This can be optimised a lot for connection reuse, etc.
async fn fetch_file_task(fud: Arc<Fud>, executor: Arc<Executor<'_>>) -> Result<()> {
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
            let session_out = fud.p2p.session_outbound();
            let session_weak = Arc::downgrade(&fud.p2p.session_outbound());

            info!("Connecting to {} to fetch {}", peer, file_hash);
            let connector = Connector::new(fud.p2p.settings(), session_weak);
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
async fn fetch_chunk_task(fud: Arc<Fud>, executor: Arc<Executor<'_>>) -> Result<()> {
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
            let session_out = fud.p2p.session_outbound();
            let session_weak = Arc::downgrade(&fud.p2p.session_outbound());

            info!("Connecting to {} to fetch {}", peer, chunk_hash);
            let connector = Connector::new(fud.p2p.settings(), session_weak);
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
            fud.chunk_fetch_tx
                .send((chunk_hash, Err(Error::GeodeChunkRouteNotFound)))
                .await
                .unwrap();
            continue
        }

        info!("Successfully fetched {} chunk", chunk_hash);
        fud.chunk_fetch_tx.send((chunk_hash, Ok(()))).await.unwrap();
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
    // The working directory for this daemon and geode.
    let basedir = expand_path(&args.base_dir)?;

    // Hashmaps used for routing
    let metadata_router = Arc::new(RwLock::new(HashMap::new()));
    let chunks_router = Arc::new(RwLock::new(HashMap::new()));

    info!("Instantiating Geode instance");
    let geode = Geode::new(&basedir).await?;

    info!("Instantiating P2P network");
    let p2p = P2p::new(args.net.into(), ex.clone()).await;

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
        rpc_connections: Mutex::new(HashSet::new()),
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

    info!(target: "fud", "Starting JSON-RPC server on {}", args.rpc_listen);
    let rpc_task = StoppableTask::new();
    let fud_ = fud.clone();
    rpc_task.clone().start(
        listen_and_serve(args.rpc_listen, fud.clone(), None, ex.clone()),
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
        .register(net::SESSION_NET, move |channel, p2p| {
            let fud_ = fud_.clone();
            async move { ProtocolFud::init(fud_, channel, p2p).await.unwrap() }
        })
        .await;
    p2p.clone().start().await?;

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

    info!("Stopping P2P network");
    p2p.stop().await;

    info!("Bye!");
    Ok(())
}
