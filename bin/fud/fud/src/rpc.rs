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

use async_trait::async_trait;
use log::{error, info};
use smol::{
    fs::{self, File},
    lock::MutexGuard,
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};
use tinyjson::JsonValue;

use darkfi::{
    dht::DhtHandler,
    geode::hash_to_string,
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        p2p_method::HandlerP2p,
        server::RequestHandler,
    },
    system::StoppableTaskPtr,
    util::path::expand_path,
    Error, Result,
};

use crate::{
    event::{self, FudEvent},
    proto::FudAnnounce,
    resource::{Resource, ResourceStatus},
    Fud,
};

#[async_trait]
impl RequestHandler<()> for Fud {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        return match req.method.as_str() {
            "ping" => self.pong(req.id, req.params).await,

            "put" => self.put(req.id, req.params).await,
            "get" => self.get(req.id, req.params).await,
            "subscribe" => self.subscribe(req.id, req.params).await,
            "remove" => self.remove_resource(req.id, req.params).await,
            "list_resources" => self.list_resources(req.id, req.params).await,
            "list_buckets" => self.list_buckets(req.id, req.params).await,
            "list_seeders" => self.list_seeders(req.id, req.params).await,
            "verify" => self.verify(req.id, req.params).await,

            "dnet.switch" => self.dnet_switch(req.id, req.params).await,
            "dnet.subscribe_events" => self.dnet_subscribe_events(req.id, req.params).await,
            "p2p.get_info" => self.p2p_get_info(req.id, req.params).await,
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
        let self_node = self.dht.node().await;

        if self_node.addresses.is_empty() {
            error!(target: "fud::put()", "Cannot put file, you don't have any external address");
            return JsonError::new(
                ErrorCode::InternalError,
                Some("You don't have any external address".to_string()),
                id,
            )
            .into()
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
                let error_str = format!("Failed inserting file {:?} to geode: {}", path, e);
                error!(target: "fud::put()", "{}", error_str);
                return JsonError::new(ErrorCode::InternalError, Some(error_str), id).into()
            }
        };

        // Add path to the sled db
        if let Err(e) = self
            .path_tree
            .insert(file_hash.as_bytes(), path.to_string_lossy().to_string().as_bytes())
        {
            error!(target: "fud::put()", "Failed inserting new file into sled: {}", e);
            return JsonError::new(ErrorCode::InternalError, None, id).into()
        }

        // Add resource
        let mut resources_write = self.resources.write().await;
        resources_write.insert(
            file_hash,
            Resource {
                hash: file_hash,
                path,
                status: ResourceStatus::Seeding,
                chunks_total: chunk_hashes.len() as u64,
                chunks_downloaded: chunk_hashes.len() as u64,
            },
        );
        drop(resources_write);

        // Announce file
        let fud_announce = FudAnnounce { key: file_hash, seeders: vec![self_node.into()] };
        let _ = self.announce(&file_hash, &fud_announce, self.seeders_router.clone()).await;

        JsonResponse::new(JsonValue::String(hash_to_string(&file_hash)), id).into()
    }

    // RPCAPI:
    // Fetch a file from the network. Takes a file hash and file path (absolute or relative) as parameters.
    // Returns the path where the file will be located once downloaded.
    //
    // --> {"jsonrpc": "2.0", "method": "get", "params": ["1211...abfd", "~/myfile.jpg"], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "/home/user/myfile.jpg", "id": 42}
    async fn get(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 2 || !params[0].is_string() || !params[1].is_string() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let mut hash_buf = vec![];
        match bs58::decode(params[0].get::<String>().unwrap().as_str()).onto(&mut hash_buf) {
            Ok(_) => {}
            Err(_) => return JsonError::new(ErrorCode::InvalidParams, None, id).into(),
        }

        if hash_buf.len() != 32 {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let mut hash_buf_arr = [0u8; 32];
        hash_buf_arr.copy_from_slice(&hash_buf);

        let file_hash = blake3::Hash::from_bytes(hash_buf_arr);
        let file_hash_str = hash_to_string(&file_hash);

        let file_path = match params[1].get::<String>() {
            Some(path) => match path.is_empty() {
                true => self.downloads_path.join(&file_hash_str).join(&file_hash_str),
                false => match PathBuf::from(path).is_absolute() {
                    true => PathBuf::from(path),
                    false => self.downloads_path.join(&file_hash_str).join(path),
                },
            },
            None => self.downloads_path.join(&file_hash_str).join(&file_hash_str),
        };

        // Get the parent directory of the file
        if let Some(parent) = file_path.parent() {
            // Create all directories leading up to the file
            let _ = fs::create_dir_all(parent).await;
        }

        let _ = self.get_tx.send((id, file_hash, file_path.clone(), Ok(()))).await;

        JsonResponse::new(JsonValue::String(file_path.to_string_lossy().to_string()), id).into()
    }

    // RPCAPI:
    // Subscribe to download events.
    //
    // --> {"jsonrpc": "2.0", "method": "get", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": `event`, "id": 42}
    async fn subscribe(&self, _id: u16, _params: JsonValue) -> JsonResult {
        self.event_sub.clone().into()
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
    // Returns resources.
    //
    // --> {"jsonrpc": "2.0", "method": "list_buckets", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [[["abcdef", ["tcp://127.0.0.1:13337"]]]], "id": 1}
    pub async fn list_resources(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let resources_read = self.resources.read().await;
        let mut resources: Vec<JsonValue> = vec![];
        for (_, resource) in resources_read.iter() {
            resources.push(resource.clone().into());
        }

        JsonResponse::new(JsonValue::Array(resources), id).into()
    }

    // RPCAPI:
    // Returns the current buckets.
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
                    JsonValue::String(hash_to_string(&node.id)),
                    JsonValue::Array(addresses),
                ]));
            }
            buckets.push(JsonValue::Array(nodes));
        }

        JsonResponse::new(JsonValue::Array(buckets), id).into()
    }

    // RPCAPI:
    // Returns the content of the seeders router.
    //
    // --> {"jsonrpc": "2.0", "method": "list_seeders", "params": [], "id": 1}
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
                node_ids.push(JsonValue::String(hash_to_string(&item.node.id)));
            }
            seeders_router.insert(hash_to_string(hash), JsonValue::Array(node_ids));
        }
        let mut res: HashMap<String, JsonValue> = HashMap::new();
        res.insert("seeders".to_string(), JsonValue::Object(seeders_router));

        JsonResponse::new(JsonValue::Object(res), id).into()
    }

    // RPCAPI:
    // Removes a resource.
    //
    // --> {"jsonrpc": "2.0", "method": "remove", "params": ["1211...abfd"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [], "id": 1}
    pub async fn remove_resource(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }
        let mut hash_buf = [0u8; 32];
        match bs58::decode(params[0].get::<String>().unwrap().as_str()).onto(&mut hash_buf) {
            Ok(_) => {}
            Err(_) => return JsonError::new(ErrorCode::InvalidParams, None, id).into(),
        }

        let hash = blake3::Hash::from_bytes(hash_buf);
        let mut resources_write = self.resources.write().await;
        resources_write.remove(&hash);
        drop(resources_write);

        self.event_publisher
            .notify(FudEvent::ResourceRemoved(event::ResourceRemoved { hash }))
            .await;

        JsonResponse::new(JsonValue::Array(vec![]), id).into()
    }

    // RPCAPI:
    // Verifies local files. Takes a list of file hashes as parameters.
    // An empty list means all known files.
    // Returns the path where the file will be located once downloaded.
    //
    // --> {"jsonrpc": "2.0", "method": "verify", "params": ["1211...abfd"], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": [], "id": 1}
    async fn verify(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.iter().all(|param| param.is_string()) {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }
        let hashes = if params.is_empty() {
            None
        } else {
            let hashes_str: Vec<String> =
                params.iter().map(|param| param.get::<String>().unwrap().clone()).collect();
            let hashes: Result<Vec<blake3::Hash>> = hashes_str
                .into_iter()
                .map(|hash_str| {
                    let mut buf = [0u8; 32];
                    bs58::decode(hash_str).onto(&mut buf)?;
                    Ok(blake3::Hash::from_bytes(buf))
                })
                .collect();
            if hashes.is_err() {
                return JsonError::new(ErrorCode::InvalidParams, None, id).into();
            }
            Some(hashes.unwrap())
        };

        if let Err(e) = self.verify_resources(hashes).await {
            error!(target: "fud::verify()", "Could not verify resources: {}", e);
            return JsonError::new(ErrorCode::InternalError, None, id).into();
        }

        JsonResponse::new(JsonValue::Array(vec![]), id).into()
    }
}

impl Fud {
    /// Handle `get` RPC request
    pub async fn handle_get(&self, file_hash: &blake3::Hash, file_path: &PathBuf) -> Result<()> {
        let self_node = self.dht().node().await;
        let mut closest_nodes = vec![];

        // Add path to the sled db
        self.path_tree
            .insert(file_hash.as_bytes(), file_path.to_string_lossy().to_string().as_bytes())?;

        // Add resource to `self.resources`
        let resource = Resource {
            hash: *file_hash,
            path: file_path.clone(),
            status: ResourceStatus::Discovering,
            chunks_total: 0,
            chunks_downloaded: 0,
        };
        let mut resources_write = self.resources.write().await;
        resources_write.insert(*file_hash, resource.clone());
        drop(resources_write);

        // Send a DownloadStarted event
        self.event_publisher
            .notify(FudEvent::DownloadStarted(event::DownloadStarted {
                hash: *file_hash,
                resource,
            }))
            .await;

        // Try to get the chunked file from geode
        let chunked_file = match self.geode.get(file_hash, file_path).await {
            // We already know the list of chunk hashes for this file
            Ok(v) => v,
            // The metadata in geode is invalid or corrupted
            Err(Error::GeodeNeedsGc) => todo!(),
            // If we could not find the file in geode, get the file metadata from the network
            Err(Error::GeodeFileNotFound) => {
                // Find nodes close to the file hash
                info!(target: "self::get()", "Requested file {} not found in Geode, triggering fetch", hash_to_string(file_hash));
                closest_nodes = self.lookup_nodes(file_hash).await.unwrap_or_default();

                // Fetch file metadata (list of chunk hashes)
                self.file_fetch_tx
                    .send((closest_nodes.clone(), *file_hash, file_path.clone(), Ok(())))
                    .await
                    .unwrap();
                info!(target: "self::get()", "Waiting for background file fetch task...");
                let (i_file_hash, status) = self.file_fetch_end_rx.recv().await.unwrap();
                match status {
                    // The file metadata was found and inserted into geode
                    Ok(()) => self.geode.get(&i_file_hash, file_path).await.unwrap(),
                    // We could not find the file metadata
                    Err(Error::GeodeFileRouteNotFound) => {
                        // Set resource status to `Incomplete` and send FudEvent::FileNotFound
                        let mut resources_write = self.resources.write().await;
                        if let Some(resource) = resources_write.get_mut(file_hash) {
                            resource.status = ResourceStatus::Incomplete;

                            self.event_publisher
                                .notify(FudEvent::FileNotFound(event::FileNotFound {
                                    hash: *file_hash,
                                    resource: resource.clone(),
                                }))
                                .await;
                        }
                        drop(resources_write);
                        return Err(Error::GeodeFileRouteNotFound);
                    }

                    Err(e) => {
                        error!(target: "fud::handle_get()", "{}", e);
                        return Err(e);
                    }
                }
            }

            Err(e) => {
                error!(target: "fud::handle_get()", "{}", e);
                return Err(e);
            }
        };

        // Set resource status to `Downloading`
        let mut resources_write = self.resources.write().await;
        let resource = match resources_write.get_mut(file_hash) {
            Some(resource) => {
                resource.status = ResourceStatus::Downloading;
                resource.chunks_downloaded = chunked_file.local_chunks() as u64;
                resource.chunks_total = chunked_file.len() as u64;
                resource.clone()
            }
            None => return Ok(()), // Resource was removed, abort
        };
        drop(resources_write);

        // Send a FileDownloadCompleted event
        self.event_publisher
            .notify(FudEvent::FileDownloadCompleted(event::FileDownloadCompleted {
                hash: *file_hash,
                resource: resource.clone(),
            }))
            .await;

        // If the file is already complete, we don't need to download any chunk
        if chunked_file.is_complete() {
            // Announce the file
            let self_announce =
                FudAnnounce { key: *file_hash, seeders: vec![self_node.clone().into()] };
            let _ = self.announce(file_hash, &self_announce, self.seeders_router.clone()).await;

            // Set resource status to `Seeding`
            let mut resources_write = self.resources.write().await;
            let resource = match resources_write.get_mut(file_hash) {
                Some(resource) => {
                    resource.status = ResourceStatus::Seeding;
                    resource.chunks_downloaded = chunked_file.len() as u64;
                    resource.clone()
                }
                None => return Ok(()), // Resource was removed, abort
            };
            drop(resources_write);

            // Send a DownloadCompleted event
            self.event_publisher
                .notify(FudEvent::DownloadCompleted(event::DownloadCompleted {
                    hash: *file_hash,
                    resource,
                }))
                .await;

            return Ok(());
        }

        // Find nodes close to the file hash if we didn't previously fetched them
        if closest_nodes.is_empty() {
            closest_nodes = self.lookup_nodes(file_hash).await.unwrap_or_default();
        }

        // Find seeders and remove ourselves from the result
        let seeders = self
            .fetch_seeders(&closest_nodes, file_hash)
            .await
            .iter()
            .filter(|seeder| seeder.node.id != self_node.id)
            .cloned()
            .collect();

        // List missing chunks
        let mut missing_chunks = HashSet::new();
        for (chunk, path) in chunked_file.iter() {
            if path.is_none() {
                missing_chunks.insert(*chunk);
            }
        }

        // Fetch missing chunks from seeders
        self.fetch_chunks(file_path, file_hash, &missing_chunks, &seeders).await?;

        // Get chunked file from geode
        let chunked_file = match self.geode.get(file_hash, file_path).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "fud::handle_get()", "{}", e);
                return Err(e);
            }
        };

        // We fetched all chunks, but the file is not complete
        // (some chunks were missing from all seeders)
        if !chunked_file.is_complete() {
            // Set resource status to `Incomplete`
            let mut resources_write = self.resources.write().await;
            let resource = match resources_write.get_mut(file_hash) {
                Some(resource) => {
                    resource.status = ResourceStatus::Incomplete;
                    resource.clone()
                }
                None => return Ok(()), // Resource was removed, abort
            };
            drop(resources_write);

            // Send a MissingChunks event
            self.event_publisher
                .notify(FudEvent::MissingChunks(event::MissingChunks {
                    hash: *file_hash,
                    resource,
                }))
                .await;
            return Ok(());
        }

        // Announce the file
        let self_announce =
            FudAnnounce { key: *file_hash, seeders: vec![self_node.clone().into()] };
        let _ = self.announce(file_hash, &self_announce, self.seeders_router.clone()).await;

        // Set resource status to `Seeding`
        let mut resources_write = self.resources.write().await;
        let resource = match resources_write.get_mut(file_hash) {
            Some(resource) => {
                resource.status = ResourceStatus::Seeding;
                resource.chunks_downloaded = chunked_file.len() as u64;
                resource.clone()
            }
            None => return Ok(()), // Resource was removed, abort
        };
        drop(resources_write);

        // Send a DownloadCompleted event
        self.event_publisher
            .notify(FudEvent::DownloadCompleted(event::DownloadCompleted {
                hash: *file_hash,
                resource,
            }))
            .await;

        Ok(())
    }
}
