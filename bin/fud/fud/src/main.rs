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

use std::{collections::HashSet, fs, path::PathBuf};

use async_std::sync::Arc;
use async_trait::async_trait;
use darkfi_serial::serialize;
use log::{debug, error, info, warn};
use serde_json::{json, Value};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize, cli_desc,
    dht::{waiting_for_response, Dht, DhtPtr},
    net,
    rpc::{
        jsonrpc::{
            ErrorCode::{InvalidParams, MethodNotFound},
            JsonError, JsonRequest, JsonResponse, JsonResult,
        },
        server::{listen_and_serve, RequestHandler},
    },
    util::path::expand_path,
    Result,
};

mod error;
use error::{server_error, RpcError};

const CONFIG_FILE: &str = "fud_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../fud_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "fud", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long, default_value = "~/.config/darkfi/fud")]
    /// Path to the contents directory
    folder: String,

    #[structopt(long, default_value = "tcp://127.0.0.1:13336")]
    /// JSON-RPC listen URL
    rpc_listen: Url,

    #[structopt(long)]
    /// P2P accept addresses (repeatable flag)
    p2p_accept: Vec<Url>,

    #[structopt(long)]
    /// P2P external addresses (repeatable flag)
    p2p_external: Vec<Url>,

    #[structopt(long, default_value = "8")]
    /// Connection slots
    slots: usize,

    #[structopt(long)]
    /// Connect to seed (repeatable flag)
    seeds: Vec<Url>,

    #[structopt(long)]
    /// Connect to peer (repeatable flag)
    peers: Vec<Url>,

    #[structopt(long)]
    /// Prefered transports for outbound connections (repeatable flag)
    transports: Vec<String>,

    #[structopt(long)]
    /// Enable localnet hosts
    localnet: bool,

    #[structopt(long)]
    /// Enable channel log
    channel_log: bool,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

/// Struct representing the daemon.
pub struct Fud {
    /// Daemon dht state
    dht: DhtPtr,

    /// Path to the contents directory
    folder: PathBuf,
}

impl Fud {
    pub async fn new(dht: DhtPtr, folder: PathBuf) -> Result<Self> {
        Ok(Self { dht, folder })
    }

    /// Initialize fud dht state by reading the contents folder and generating
    /// the corresponding dht records.
    async fn init(&self) -> Result<()> {
        info!("Initializing fud dht state for folder: {:?}", self.folder);

        if !self.folder.exists() {
            fs::create_dir_all(&self.folder)?;
        }

        let entries = fs::read_dir(&self.folder).unwrap();
        {
            let mut lock = self.dht.write().await;

            // Sync lookup map with network
            if let Err(e) = lock.sync_lookup_map().await {
                error!("Failed to sync lookup map: {}", e);
            }

            for entry in entries {
                let e = entry.unwrap();
                let name = String::from(e.file_name().to_str().unwrap());
                info!("Entry: {}", name);
                let key_hash = blake3::hash(&serialize(&name));
                let value: Vec<u8> = std::fs::read(e.path()).unwrap();
                if let Err(e) = lock.insert(key_hash, value).await {
                    error!("Failed to insert key: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Signaling fud network that node goes offline.
    async fn disconnect(&self) -> Result<()> {
        debug!("Peer disconnecting, signaling network");

        {
            let mut lock = self.dht.write().await;
            let records = lock.map.clone();
            for key in records.keys() {
                let result = lock.remove(*key).await;
                match result {
                    Ok(option) => match option {
                        Some(k) => {
                            debug!("Hash key removed: {}", k);
                        }
                        None => {
                            warn!("Did not find key: {}", key);
                        }
                    },
                    Err(e) => {
                        error!("Failed to remove key: {}", e);
                    }
                }
            }
        }

        Ok(())
    }

    // RPCAPI:
    // Returns all folder contents, with file changes.
    // --> {"jsonrpc": "2.0", "method": "list", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "[[files],[new],[deleted]", "id": 1}
    pub async fn list(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut content = HashSet::new();
        let mut new = HashSet::new();
        let mut deleted = HashSet::new();

        let entries = fs::read_dir(&self.folder).unwrap();
        let records = self.dht.read().await.map.clone();
        let mut entries_hashes = HashSet::new();

        // We iterate files for new records
        for entry in entries {
            let e = entry.unwrap();
            let name = String::from(e.file_name().to_str().unwrap());
            let key_hash = blake3::hash(&serialize(&name));
            entries_hashes.insert(key_hash);

            if records.contains_key(&key_hash) {
                content.insert(name.clone());
            } else {
                new.insert(name);
            }
        }

        // We check records for removed files
        for key in records.keys() {
            if entries_hashes.contains(key) {
                continue
            }
            deleted.insert(key.to_string());
        }

        JsonResponse::new(json!((content, new, deleted)), id).into()
    }

    // RPCAPI:
    // Iterate contents folder and dht for potential changes.
    // --> {"jsonrpc": "2.0", "method": "sync", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "true", "id": 1}
    pub async fn sync(&self, id: Value, _params: &[Value]) -> JsonResult {
        info!("Sync process started");

        let entries = fs::read_dir(&self.folder).unwrap();
        {
            let mut lock = self.dht.write().await;
            let records = lock.map.clone();
            let mut entries_hashes = HashSet::new();

            // We iterate files for new records
            for entry in entries {
                let e = entry.unwrap();
                let name = String::from(e.file_name().to_str().unwrap());
                info!("Entry: {}", name);
                let key_hash = blake3::hash(&serialize(&name));
                entries_hashes.insert(key_hash);

                if records.contains_key(&key_hash) {
                    continue
                }

                let value: Vec<u8> = std::fs::read(e.path()).unwrap();
                if let Err(e) = lock.insert(key_hash, value).await {
                    error!("Failed to insert key: {}", e);
                    return server_error(RpcError::KeyInsertFail, id)
                }
            }

            // We check records for removed files
            let records = lock.map.clone();
            for key in records.keys() {
                if entries_hashes.contains(key) {
                    continue
                }

                let result = lock.remove(*key).await;
                match result {
                    Ok(option) => match option {
                        Some(k) => {
                            debug!("Hash key removed: {}", k);
                        }
                        None => {
                            warn!("Did not find key: {}", key);
                        }
                    },
                    Err(e) => {
                        error!("Failed to remove key: {}", e);
                        return server_error(RpcError::KeyRemoveFail, id)
                    }
                }
            }
        }

        JsonResponse::new(json!(true), id).into()
    }

    // RPCAPI:
    // Checks if provided key exists and retrieve it from the local map or queries the network.
    // Returns key or not found message.
    // --> {"jsonrpc": "2.0", "method": "get", "params": ["name"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "path", "id": 1}
    async fn get(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let key = params[0].as_str().unwrap().to_string();
        let key_hash = blake3::hash(&serialize(&key));

        // We execute this sequence to prevent lock races between threads
        // Verify key exists
        let exists = self.dht.read().await.contains_key(key_hash);
        if exists.is_none() {
            info!("Did not find key: {}", key);
            return server_error(RpcError::UnknownKey, id)
        }

        // Check if key is local or should query network
        let path = self.folder.join(key.clone());
        let local = exists.unwrap();
        if local {
            match self.dht.read().await.get(key_hash) {
                Some(_) => return JsonResponse::new(json!(path), id).into(),
                None => {
                    info!("Did not find key: {}", key);
                    return server_error(RpcError::UnknownKey, id)
                }
            }
        }

        info!("Key doesn't exist locally, querring network...");
        if let Err(e) = self.dht.read().await.request_key(key_hash).await {
            error!("Failed to query key: {}", e);
            return server_error(RpcError::QueryFailed, id)
        }

        info!("Waiting response...");
        match waiting_for_response(self.dht.clone()).await {
            Ok(response) => {
                match response {
                    Some(resp) => {
                        info!("Key found!");
                        // Optionally, we insert the key to our local map
                        if let Err(e) =
                            self.dht.write().await.insert(resp.key, resp.value.clone()).await
                        {
                            error!("Failed to insert key: {}", e);
                            return server_error(RpcError::KeyInsertFail, id)
                        }

                        if let Err(e) = std::fs::write(path.clone(), resp.value) {
                            error!("Failed to generate file for key: {}", e);
                            return server_error(RpcError::FileGenerationFail, id)
                        }
                        JsonResponse::new(json!(path), id).into()
                    }
                    None => {
                        info!("Did not find key: {}", key);
                        server_error(RpcError::UnknownKey, id)
                    }
                }
            }
            Err(e) => {
                error!("Error while waiting network response: {}", e);
                server_error(RpcError::WaitingNetworkError, id)
            }
        }
    }

    // RPCAPI:
    // Replies to a ping method.
    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 42}
    async fn pong(&self, id: Value, _params: &[Value]) -> JsonResult {
        JsonResponse::new(json!("pong"), id).into()
    }

    // RPCAPI:
    // Retrieves P2P network information.
    // --> {"jsonrpc": "2.0", "method": "get_info", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", result": {"nodeID": [], "nodeinfo": [], "id": 42}
    async fn get_info(&self, id: Value, _params: &[Value]) -> JsonResult {
        let resp = self.dht.read().await.p2p.get_info().await;
        JsonResponse::new(resp, id).into()
    }
}

#[async_trait]
impl RequestHandler for Fud {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return JsonError::new(InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        match req.method.as_str() {
            Some("list") => return self.list(req.id, params).await,
            Some("sync") => return self.sync(req.id, params).await,
            Some("get") => return self.get(req.id, params).await,
            Some("ping") => return self.pong(req.id, params).await,
            Some("get_info") => return self.get_info(req.id, params).await,
            Some(_) | None => return JsonError::new(MethodNotFound, None, req.id).into(),
        }
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'_>>) -> Result<()> {
    // We use this handler to block this function after detaching all
    // tasks, and to catch a shutdown signal, where we can clean up and
    // exit gracefully.
    let (signal, shutdown) = smol::channel::bounded::<()>(1);
    ctrlc::set_handler(move || {
        async_std::task::block_on(signal.send(())).unwrap();
    })
    .unwrap();

    // P2P network
    let network_settings = net::Settings {
        inbound_addrs: args.p2p_accept,
        outbound_connections: args.slots,
        external_addrs: args.p2p_external,
        peers: args.peers.clone(),
        seeds: args.seeds.clone(),
        allowed_transports: args.transports,
        localnet: args.localnet,
        ..Default::default()
    };

    let p2p = net::P2p::new(network_settings).await;

    // Initialize daemon dht
    let dht = Dht::new(None, p2p.clone(), shutdown.clone(), ex.clone()).await?;

    // Initialize daemon
    let folder = expand_path(&args.folder)?;
    let fud = Fud::new(dht.clone(), folder).await?;
    let fud = Arc::new(fud);

    // JSON-RPC server
    info!("Starting JSON-RPC server");
    let _ex = ex.clone();
    ex.spawn(listen_and_serve(args.rpc_listen, fud.clone(), _ex)).detach();

    info!("Starting sync P2P network");
    p2p.clone().start(ex.clone()).await?;
    let _ex = ex.clone();
    let _p2p = p2p.clone();
    ex.spawn(async move {
        if let Err(e) = _p2p.run(_ex).await {
            error!("Failed starting P2P network: {}", e);
        }
    })
    .detach();

    fud.init().await?;

    // Wait for SIGINT
    shutdown.recv().await?;
    print!("\r");
    info!("Caught termination signal, cleaning up and exiting...");

    fud.disconnect().await?;

    Ok(())
}
