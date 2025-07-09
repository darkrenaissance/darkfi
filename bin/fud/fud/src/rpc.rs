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
use smol::lock::{Mutex, MutexGuard};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};
use tinyjson::JsonValue;
use tracing::error;

use darkfi::{
    dht::DhtNode,
    geode::hash_to_string,
    net::P2pPtr,
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult, JsonSubscriber},
        p2p_method::HandlerP2p,
        server::RequestHandler,
    },
    system::StoppableTaskPtr,
    util::path::expand_path,
    Result,
};

use crate::{util::FileSelection, Fud};

pub struct JsonRpcInterface {
    fud: Arc<Fud>,
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    dnet_sub: JsonSubscriber,
    event_sub: JsonSubscriber,
}

#[async_trait]
impl RequestHandler<()> for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        return match req.method.as_str() {
            "ping" => self.pong(req.id, req.params).await,

            "put" => self.put(req.id, req.params).await,
            "get" => self.get(req.id, req.params).await,
            "subscribe" => self.subscribe(req.id, req.params).await,
            "remove" => self.remove(req.id, req.params).await,
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

impl HandlerP2p for JsonRpcInterface {
    fn p2p(&self) -> P2pPtr {
        self.fud.p2p.clone()
    }
}

/// Fud RPC methods
impl JsonRpcInterface {
    pub fn new(fud: Arc<Fud>, dnet_sub: JsonSubscriber, event_sub: JsonSubscriber) -> Self {
        Self { fud, rpc_connections: Mutex::new(HashSet::new()), dnet_sub, event_sub }
    }

    // RPCAPI:
    // Put a file/directory onto the network. Takes a local filesystem path as a parameter.
    // Returns the resource hash that serves as a pointer to the file/directory.
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
        let res = self.fud.put(&path).await;
        if let Err(e) = res {
            return JsonError::new(ErrorCode::InternalError, Some(format!("{e}")), id).into()
        }

        JsonResponse::new(JsonValue::String(path.to_string_lossy().to_string()), id).into()
    }

    // RPCAPI:
    // Fetch a resource from the network. Takes a hash, path (absolute or relative), and an
    // optional list of file paths (only used for directories) as parameters.
    // Returns the path where the resource will be located once downloaded.
    //
    // --> {"jsonrpc": "2.0", "method": "get", "params": ["1211...abfd", "~/myfile.jpg", null], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "/home/user/myfile.jpg", "id": 42}
    async fn get(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 3 || !params[0].is_string() || !params[1].is_string() {
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

        let hash = blake3::Hash::from_bytes(hash_buf_arr);
        let hash_str = hash_to_string(&hash);

        let path = match params[1].get::<String>() {
            Some(path) => match path.is_empty() {
                true => match self.fud.hash_to_path(&hash).ok().flatten() {
                    Some(path) => path,
                    None => self.fud.downloads_path.join(&hash_str),
                },
                false => match PathBuf::from(path).is_absolute() {
                    true => PathBuf::from(path),
                    false => self.fud.downloads_path.join(path),
                },
            },
            None => self.fud.downloads_path.join(&hash_str),
        };

        let files: FileSelection = match &params[2] {
            JsonValue::Array(files) => files
                .iter()
                .filter_map(|v| {
                    if let JsonValue::String(file) = v {
                        Some(PathBuf::from(file.clone()))
                    } else {
                        None
                    }
                })
                .collect(),
            JsonValue::Null => FileSelection::All,
            _ => return JsonError::new(ErrorCode::InvalidParams, None, id).into(),
        };

        // Start downloading the resource
        if let Err(e) = self.fud.get(&hash, &path, files).await {
            return JsonError::new(ErrorCode::InternalError, Some(e.to_string()), id).into()
        }

        JsonResponse::new(JsonValue::String(path.to_string_lossy().to_string()), id).into()
    }

    // RPCAPI:
    // Subscribe to fud events.
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
            self.fud.p2p.dnet_enable();
        } else {
            self.fud.p2p.dnet_disable();
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

        let resources_read = self.fud.resources.read().await;
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
    // <-- {"jsonrpc": "2.0", "result": [["abcdef", ["tcp://127.0.0.1:13337"]]], "id": 1}
    pub async fn list_buckets(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }
        let mut buckets = vec![];
        for bucket in self.fud.dht.buckets.read().await.iter() {
            let mut nodes = vec![];
            for node in bucket.nodes.clone() {
                let mut addresses = vec![];
                for addr in &node.addresses {
                    addresses.push(JsonValue::String(addr.to_string()));
                }
                nodes.push(JsonValue::Array(vec![
                    JsonValue::String(hash_to_string(&node.id())),
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
    // <-- {"jsonrpc": "2.0", "result": {"seeders": {"abcdefileid": [["abcdef", ["tcp://127.0.0.1:13337"]]]}}, "id": 1}
    pub async fn list_seeders(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }
        let mut seeders_table: HashMap<String, JsonValue> = HashMap::new();
        for (hash, items) in self.fud.dht.hash_table.read().await.iter() {
            let mut nodes = vec![];
            for item in items {
                let mut addresses = vec![];
                for addr in &item.node.addresses {
                    addresses.push(JsonValue::String(addr.to_string()));
                }
                nodes.push(JsonValue::Array(vec![
                    JsonValue::String(hash_to_string(&item.node.id())),
                    JsonValue::Array(addresses),
                ]));
            }
            seeders_table.insert(hash_to_string(hash), JsonValue::Array(nodes));
        }
        let mut res: HashMap<String, JsonValue> = HashMap::new();
        res.insert("seeders".to_string(), JsonValue::Object(seeders_table));

        JsonResponse::new(JsonValue::Object(res), id).into()
    }

    // RPCAPI:
    // Removes a resource.
    //
    // --> {"jsonrpc": "2.0", "method": "remove", "params": ["1211...abfd"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [], "id": 1}
    pub async fn remove(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }
        let mut hash_buf = [0u8; 32];
        match bs58::decode(params[0].get::<String>().unwrap().as_str()).onto(&mut hash_buf) {
            Ok(_) => {}
            Err(_) => return JsonError::new(ErrorCode::InvalidParams, None, id).into(),
        }

        self.fud.remove(&blake3::Hash::from_bytes(hash_buf)).await;

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

        if let Err(e) = self.fud.verify_resources(hashes).await {
            error!(target: "fud::verify()", "Could not verify resources: {e}");
            return JsonError::new(ErrorCode::InternalError, None, id).into();
        }

        JsonResponse::new(JsonValue::Array(vec![]), id).into()
    }
}
