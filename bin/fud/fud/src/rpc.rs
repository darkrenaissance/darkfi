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
    path::PathBuf,
};

use crate::{dht::DhtHandler, proto::FudAnnounce, Fud};
use async_trait::async_trait;
use darkfi::{
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        p2p_method::HandlerP2p,
        server::RequestHandler,
        util::{json_map, json_str},
    },
    system::StoppableTaskPtr,
    util::path::expand_path,
    Error,
};
use log::{error, info};
use smol::{fs::File, lock::MutexGuard};
use tinyjson::JsonValue;

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

/// Fud RPC methods
impl Fud {
    // RPCAPI:
    // Put a file onto the network. Takes a local filesystem path as a parameter.
    // Returns the file hash that serves as a pointer to the uploaded file.
    //
    // --> {"jsonrpc": "2.0", "method": "put", "params": ["/foo.txt"], "id": 42}
    // <-- {"jsonrpc": "2.0", "result: "df4...3db7", "id": 42}
    async fn put(&self, id: u16, params: JsonValue) -> JsonResult {
        if self.dht().node.addresses.is_empty() {
            error!(target: "fud::put()", "Cannot put file, you don't have any external address");
            return JsonError::new(ErrorCode::InternalError, None, id).into()
        }

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
                error!(target: "fud::put()", "Failed to open {:?}: {}", path, e);
                return JsonError::new(ErrorCode::InvalidParams, None, id).into()
            }
        };

        let (file_hash, chunk_hashes) = match self.geode.insert(fd).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "fud::put()", "Failed inserting file {:?} to geode: {}", path, e);
                return JsonError::new(ErrorCode::InternalError, None, id).into()
            }
        };

        // Announce file
        let self_node = self.dht.node.clone();
        let fud_announce = FudAnnounce { key: file_hash, seeders: vec![self_node.clone().into()] };
        let _ = self.announce(&file_hash, &fud_announce, self.seeders_router.clone()).await;

        // Announce chunks
        for chunk_hash in chunk_hashes {
            let fud_announce =
                FudAnnounce { key: chunk_hash, seeders: vec![self_node.clone().into()] };
            let _ = self.announce(&chunk_hash, &fud_announce, self.seeders_router.clone()).await;
        }

        JsonResponse::new(JsonValue::String(file_hash.to_hex().to_string()), id).into()
    }

    // RPCAPI:
    // Fetch a file from the network, and subscribe to download events. Takes a file hash as parameter.
    //
    // --> {"jsonrpc": "2.0", "method": "get", "params": ["1211...abfd"], "id": 42}
    // <-- {"jsonrpc": "2.0", "method": "get", "params": `event`}
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

        let file_hash = match blake3::Hash::from_hex(params[0].get::<String>().unwrap()) {
            Ok(v) => v,
            Err(_) => return JsonError::new(ErrorCode::InvalidParams, None, id).into(),
        };

        let _ = self.get_tx.send((id, file_hash, file_name, Ok(()))).await;

        self.download_sub.clone().into()
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
        for (hash, items) in self.seeders_router.read().await.iter() {
            let mut node_ids = vec![];
            for item in items {
                node_ids.push(JsonValue::String(item.node.id.to_hex().to_string()));
            }
            seeders_router.insert(hash.to_hex().to_string(), JsonValue::Array(node_ids));
        }
        let mut res: HashMap<String, JsonValue> = HashMap::new();
        res.insert("seeders".to_string(), JsonValue::Object(seeders_router));

        JsonResponse::new(JsonValue::Object(res), id).into()
    }
}

#[derive(Clone, Debug)]
pub struct ChunkDownloadCompleted {
    pub file_hash: blake3::Hash,
    pub chunk_hash: blake3::Hash,
}
#[derive(Clone, Debug)]
pub struct FileDownloadCompleted {
    pub file_hash: blake3::Hash,
    pub chunk_count: usize,
}
#[derive(Clone, Debug)]
pub struct DownloadCompleted {
    pub file_hash: blake3::Hash,
    pub file_path: PathBuf,
}
#[derive(Clone, Debug)]
pub struct ChunkNotFound {
    pub file_hash: blake3::Hash,
    pub chunk_hash: blake3::Hash,
}
#[derive(Clone, Debug)]
pub struct FileNotFound {
    pub file_hash: blake3::Hash,
}
#[derive(Clone, Debug)]
pub struct MissingChunks {}

#[derive(Clone, Debug)]
pub enum FudEvent {
    ChunkDownloadCompleted(ChunkDownloadCompleted),
    FileDownloadCompleted(FileDownloadCompleted),
    DownloadCompleted(DownloadCompleted),
    ChunkNotFound(ChunkNotFound),
    FileNotFound(FileNotFound),
    MissingChunks(MissingChunks),
}

impl From<ChunkDownloadCompleted> for JsonValue {
    fn from(info: ChunkDownloadCompleted) -> JsonValue {
        json_map([
            ("file_hash", JsonValue::String(info.file_hash.to_hex().to_string())),
            ("chunk_hash", JsonValue::String(info.chunk_hash.to_hex().to_string())),
        ])
    }
}
impl From<FileDownloadCompleted> for JsonValue {
    fn from(info: FileDownloadCompleted) -> JsonValue {
        json_map([
            ("file_hash", JsonValue::String(info.file_hash.to_hex().to_string())),
            ("chunk_count", JsonValue::Number(info.chunk_count as f64)),
        ])
    }
}
impl From<DownloadCompleted> for JsonValue {
    fn from(info: DownloadCompleted) -> JsonValue {
        json_map([
            ("file_hash", JsonValue::String(info.file_hash.to_hex().to_string())),
            ("file_path", JsonValue::String(info.file_path.to_string_lossy().to_string())),
        ])
    }
}
impl From<ChunkNotFound> for JsonValue {
    fn from(info: ChunkNotFound) -> JsonValue {
        json_map([
            ("file_hash", JsonValue::String(info.file_hash.to_hex().to_string())),
            ("chunk_hash", JsonValue::String(info.chunk_hash.to_hex().to_string())),
        ])
    }
}
impl From<FileNotFound> for JsonValue {
    fn from(info: FileNotFound) -> JsonValue {
        json_map([("file_hash", JsonValue::String(info.file_hash.to_hex().to_string()))])
    }
}
impl From<FudEvent> for JsonValue {
    fn from(event: FudEvent) -> JsonValue {
        match event {
            FudEvent::ChunkDownloadCompleted(info) => {
                json_map([("event", json_str("chunk_download_completed")), ("info", info.into())])
            }
            FudEvent::FileDownloadCompleted(info) => {
                json_map([("event", json_str("file_download_completed")), ("info", info.into())])
            }
            FudEvent::DownloadCompleted(info) => {
                json_map([("event", json_str("download_completed")), ("info", info.into())])
            }
            FudEvent::ChunkNotFound(info) => {
                json_map([("event", json_str("chunk_not_found")), ("info", info.into())])
            }
            FudEvent::FileNotFound(info) => {
                json_map([("event", json_str("file_not_found")), ("info", info.into())])
            }
            FudEvent::MissingChunks(_) => json_map([("event", json_str("missing_chunks"))]),
        }
    }
}

impl Fud {
    /// Handle `get` RPC request
    pub async fn handle_get(&self, file_hash: blake3::Hash, file_name: Option<String>) {
        let self_node = self.dht().node.clone();

        let chunked_file = match self.geode.get(&file_hash).await {
            Ok(v) => v,
            Err(Error::GeodeNeedsGc) => todo!(),
            Err(Error::GeodeFileNotFound) => {
                info!(target: "self::get()", "Requested file {} not found in Geode, triggering fetch", file_hash);
                self.file_fetch_tx.send((file_hash, Ok(()))).await.unwrap();
                info!(target: "self::get()", "Waiting for background file fetch task...");
                let (i_file_hash, status) = self.file_fetch_end_rx.recv().await.unwrap();
                match status {
                    Ok(()) => self.geode.get(&i_file_hash).await.unwrap(),

                    Err(Error::GeodeFileRouteNotFound) => {
                        self.download_publisher
                            .notify(FudEvent::FileNotFound(FileNotFound { file_hash }))
                            .await;
                        return;
                    }

                    Err(e) => panic!("{}", e),
                }
            }

            Err(e) => panic!("{}", e),
        };

        self.download_publisher
            .notify(FudEvent::FileDownloadCompleted(FileDownloadCompleted {
                file_hash,
                chunk_count: chunked_file.len(),
            }))
            .await;

        if chunked_file.is_complete() {
            let self_announce =
                FudAnnounce { key: file_hash, seeders: vec![self_node.clone().into()] };
            let _ = self.announce(&file_hash, &self_announce, self.seeders_router.clone()).await;

            return match self.geode.assemble_file(&file_hash, &chunked_file, file_name).await {
                Ok(file_path) => {
                    self.download_publisher
                        .notify(FudEvent::DownloadCompleted(DownloadCompleted {
                            file_hash,
                            file_path: file_path.clone(),
                        }))
                        .await;
                }
                Err(e) => {
                    error!(target: "fud::handle_get()", "{}", e);
                }
            };
        }

        // Fetch any missing chunks
        let mut missing_chunks = vec![];
        for (chunk, path) in chunked_file.iter() {
            if path.is_none() {
                missing_chunks.push(*chunk);
            } else {
                self.download_publisher
                    .notify(FudEvent::ChunkDownloadCompleted(ChunkDownloadCompleted {
                        file_hash,
                        chunk_hash: *chunk,
                    }))
                    .await;
            }
        }

        for chunk in missing_chunks {
            self.chunk_fetch_tx.send((chunk, Ok(()))).await.unwrap();
            let (i_chunk_hash, status) = self.chunk_fetch_end_rx.recv().await.unwrap();

            match status {
                Ok(()) => {
                    self.download_publisher
                        .notify(FudEvent::ChunkDownloadCompleted(ChunkDownloadCompleted {
                            file_hash,
                            chunk_hash: i_chunk_hash,
                        }))
                        .await;
                    let self_announce =
                        FudAnnounce { key: i_chunk_hash, seeders: vec![self_node.clone().into()] };
                    let _ = self
                        .announce(&i_chunk_hash, &self_announce, self.seeders_router.clone())
                        .await;
                }
                Err(Error::GeodeChunkRouteNotFound) => {
                    self.download_publisher
                        .notify(FudEvent::ChunkNotFound(ChunkNotFound {
                            file_hash,
                            chunk_hash: i_chunk_hash,
                        }))
                        .await;
                    return;
                }

                Err(e) => panic!("{}", e),
            };
        }

        let chunked_file = match self.geode.get(&file_hash).await {
            Ok(v) => v,
            Err(e) => panic!("{}", e),
        };

        // We fetched all chunks, but the file is not complete?
        if !chunked_file.is_complete() {
            self.download_publisher.notify(FudEvent::MissingChunks(MissingChunks {})).await;
            return;
        }

        match self.geode.assemble_file(&file_hash, &chunked_file, file_name).await {
            Ok(file_path) => {
                self.download_publisher
                    .notify(FudEvent::DownloadCompleted(DownloadCompleted {
                        file_hash,
                        file_path: file_path.clone(),
                    }))
                    .await;
            }
            Err(e) => {
                error!(target: "fud::handle_get()", "{}", e);
            }
        };
    }
}
