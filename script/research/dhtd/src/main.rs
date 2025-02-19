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

use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use futures_lite::future;
use log::{error, info};
use serde_derive::Deserialize;
use serde_json::{json, Value};
use structopt::StructOpt;
use structopt_toml::StructOptToml;
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
        settings::RpcSettingsOpt
    },
    util::{
        cli::{get_log_config, get_log_level, spawn_config},
        path::get_config_path,
        serial::serialize,
        expand_path,
    },
    Result,
};

mod error;
use error::{server_error, RpcError};
const CONFIG_FILE: &str = "dhtd_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../dhtd_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "dhtd", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(flatten)]
    /// JSON-RPC settings
    rpc: RpcSettingsOpt,

    #[structopt(long)]
    /// P2P accept addresses (repeatable flag)
    p2p_accept: Vec<Url>,

    #[structopt(long)]
    /// P2P external addresses (repeatable flag)
    p2p_external: Vec<Url>,

    #[structopt(long, default_value = "8")]
    /// Connection slots
    slots: u32,

    #[structopt(long)]
    /// Connect to seed (repeatable flag)
    p2p_seed: Vec<Url>,

    #[structopt(long)]
    /// Connect to peer (repeatable flag)
    p2p_peer: Vec<Url>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

/// Struct representing DHT daemon.
/// This example/temp-impl stores String data.
/// In final version everything will be in bytes (Vec<u8).
pub struct Dhtd {
    /// Daemon dht state
    dht: DhtPtr,
}

impl Dhtd {
    pub async fn new(dht: DhtPtr) -> Result<Self> {
        Ok(Self { dht })
    }

    // RPCAPI:
    // Checks if provided key exists and retrieve it from the local map or queries the network.
    // Returns key value or not found message.
    // --> {"jsonrpc": "2.0", "method": "get", "params": ["key"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "value", "id": 1}
    async fn get(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let key = params[0].to_string();
        let key_hash = blake3::hash(&serialize(&key));

        // We execute this sequence to prevent lock races between threads
        // Verify key exists
        let exists = self.dht.read().await.contains_key(key_hash.clone());
        if let None = exists {
            info!("Did not find key: {}", key);
            return server_error(RpcError::UnknownKey, id).into()
        }

        // Check if key is local or shoud query network
        let local = exists.unwrap();
        if local {
            return match self.dht.read().await.get(key_hash.clone()) {
                Some(value) => {
                    let string = std::str::from_utf8(&value).unwrap().to_string();
                    JsonResponse::new(json!((key, string)), id).into()
                }
                None => {
                    info!("Did not find key: {}", key);
                    server_error(RpcError::UnknownKey, id).into()
                }
            }
        }

        info!("Key doesn't exist locally, querring network...");
        if let Err(e) = self.dht.read().await.request_key(key_hash).await {
            error!("Failed to query key: {}", e);
            return server_error(RpcError::QueryFailed, id).into()
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
                        let string = std::str::from_utf8(&resp.value).unwrap().to_string();
                        JsonResponse::new(json!((key, string)), id).into()
                    }
                    None => {
                        info!("Did not find key: {}", key);
                        server_error(RpcError::UnknownKey, id).into()
                    }
                }
            }
            Err(e) => {
                error!("Error while waiting network response: {}", e);
                server_error(RpcError::WaitingNetworkError, id).into()
            }
        }
    }

    // RPCAPI:
    // Insert key value pair in dht.
    // --> {"jsonrpc": "2.0", "method": "insert", "params": ["key", "value"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "(key, value)", "id": 1}
    async fn insert(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 2 || !params[0].is_string() || !params[1].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let key = params[0].to_string();
        let key_hash = blake3::hash(&serialize(&key));
        let value = params[1].to_string();

        if let Err(e) = self.dht.write().await.insert(key_hash, value.as_bytes().to_vec()).await {
            error!("Failed to insert key: {}", e);
            return server_error(RpcError::KeyInsertFail, id)
        }

        JsonResponse::new(json!((key, value)), id).into()
    }

    // RPCAPI:
    // Remove key value pair from local map.
    // --> {"jsonrpc": "2.0", "method": "remove", "params": ["key"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "key", "id": 1}
    async fn remove(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let key = params[0].to_string();
        let key_hash = blake3::hash(&serialize(&key));

        // Check if key value pair existed and act accordingly
        let result = self.dht.write().await.remove(key_hash).await;
        match result {
            Ok(option) => match option {
                Some(k) => {
                    info!("Hash key removed: {}", k);
                    JsonResponse::new(json!(k.to_string()), id).into()
                }
                None => {
                    info!("Did not find key: {}", key);
                    server_error(RpcError::UnknownKey, id).into()
                }
            },
            Err(e) => {
                error!("Failed to remove key: {}", e);
                server_error(RpcError::KeyRemoveFail, id)
            }
        }
    }

    // RPCAPI:
    // Returns current local map.
    // --> {"jsonrpc": "2.0", "method": "map", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "map", "id": 1}
    pub async fn map(&self, id: Value, _params: &[Value]) -> JsonResult {
        let map = self.dht.read().await.map.clone();
        let map_string = format!("{:#?}", map);
        JsonResponse::new(json!(map_string), id).into()
    }

    // RPCAPI:
    // Returns current lookup map.
    // --> {"jsonrpc": "2.0", "method": "lookup", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "lookup", "id": 1}
    pub async fn lookup(&self, id: Value, _params: &[Value]) -> JsonResult {
        let lookup = self.dht.read().await.lookup.clone();
        let lookup_string = format!("{:#?}", lookup);
        JsonResponse::new(json!(lookup_string), id).into()
    }
}

#[async_trait]
impl RequestHandler<()> for Dhtd {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return JsonError::new(InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        return match req.method.as_str() {
            Some("get") => self.get(req.id, params).await,
            Some("insert") => self.insert(req.id, params).await,
            Some("remove") => self.remove(req.id, params).await,
            Some("map") => self.map(req.id, params).await,
            Some("lookup") => self.lookup(req.id, params).await,
            Some(_) | None => JsonError::new(MethodNotFound, None, req.id).into(),
        }
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'_>>) -> Result<()> {
    // We use this handler to block this function after detaching all
    // tasks, and to catch a shutdown signal, where we can clean up and
    // exit gracefully.
    let (signal, shutdown) = async_channel::bounded::<()>(1);
    ctrlc::set_handler(move || {
        async_std::task::block_on(signal.send(())).unwrap();
    })
    .unwrap();

    // P2P network
    let network_settings = net::Settings {
        inbound: args.p2p_accept,
        outbound_connections: args.slots,
        external_addr: args.p2p_external,
        peers: args.p2p_seed.clone(),
        seeds: args.p2p_seed.clone(),
        ..Default::default()
    };

    let p2p = net::P2p::new(network_settings).await;

    // Initialize daemon dht
    let dht = Dht::new(None, p2p.clone(), shutdown.clone(), ex.clone()).await?;

    // Initialize daemon
    let dhtd = Dhtd::new(dht.clone()).await?;
    let dhtd = Arc::new(dhtd);

    // JSON-RPC server
    info!("Starting JSON-RPC server");
    let _ex = ex.clone();
    ex.spawn(listen_and_serve(args.rpc.into(), dhtd.clone(), _ex)).detach();

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

    // Wait for SIGINT
    shutdown.recv().await?;
    print!("\r");
    info!("Caught termination signal, cleaning up and exiting...");

    Ok(())
}
