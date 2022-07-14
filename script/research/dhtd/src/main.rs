use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use chrono::Utc;
use futures::{select, FutureExt};
use futures_lite::future;
use log::{debug, error, info, warn};
use serde_derive::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::{
    async_daemonize, cli_desc, net,
    net::P2pPtr,
    rpc::{
        jsonrpc::{
            ErrorCode::{InvalidParams, MethodNotFound},
            JsonError, JsonRequest, JsonResponse, JsonResult,
        },
        server::{listen_and_serve, RequestHandler},
    },
    util::{
        cli::{get_log_config, get_log_level, spawn_config},
        path::get_config_path,
        sleep,
    },
    Result,
};

mod error;
use error::{server_error, RpcError};

mod structures;
use structures::{KeyRequest, KeyResponse, State, StatePtr};

mod protocol;
use protocol::Protocol;

const CONFIG_FILE: &str = "dhtd_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../dhtd_config.toml");
const REQUEST_TIMEOUT: u64 = 2400;
const SEEN_DURATION: i64 = 120;

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "dhtd", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long, default_value = "tcp://127.0.0.1:9540")]
    /// JSON-RPC listen URL
    rpc_listen: Url,

    #[structopt(long)]
    /// P2P accept address
    p2p_accept: Option<Url>,

    #[structopt(long)]
    /// P2P external address
    p2p_external: Option<Url>,

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

/// Struct representing DHT daemon state.
/// In this example we store String data.
pub struct Dhtd {
    /// Daemon state
    state: StatePtr,
    /// P2P network pointer
    p2p: P2pPtr,
    /// Channel to receive responses from P2P
    p2p_recv_channel: async_channel::Receiver<KeyResponse>,
    /// Stop signal channel to terminate background processes
    stop_signal: async_channel::Receiver<()>,
}

impl Dhtd {
    pub async fn new(
        state: StatePtr,
        p2p: P2pPtr,
        p2p_recv_channel: async_channel::Receiver<KeyResponse>,
        stop_signal: async_channel::Receiver<()>,
    ) -> Result<Self> {
        Ok(Self { state, p2p, p2p_recv_channel, stop_signal })
    }

    // RPCAPI:
    // Checks if provided key exist in local map, otherwise queries the network.
    // Returns key value or not found message.
    // --> {"jsonrpc": "2.0", "method": "get", "params": ["key"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "value", "id": 1}
    async fn get(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Each node holds a local map, acting as its cache.
        // When the node receives a request for a key it doesn't hold,
        // it will query the P2P network and saves the response in its local cache.
        let key = params[0].to_string();
        match self.state.read().await.map.get(&key) {
            Some(v) => {
                let string = std::str::from_utf8(&v).unwrap();
                return JsonResponse::new(json!(string), id).into()
            }
            None => info!("Requested key doesn't exist, querying the network..."),
        };

        // We retrieve p2p network connected channels, to verify if we
        // are connected to a network.
        // Using len here because is_empty() uses unstable library feature
        // called 'exact_size_is_empty'.
        if self.p2p.channels().lock().await.values().len() == 0 {
            warn!("Node is not connected to other nodes");
            return server_error(RpcError::UnknownKey, id).into()
        }

        // We create a key request, and broadcast it to the network
        // TODO: this should be based on the lookup table, and ask peers directly
        let daemon = self.state.read().await.id.to_string();
        let request = KeyRequest::new(daemon, key.clone());
        if let Err(e) = self.p2p.broadcast(request).await {
            error!("Failed broadcasting request: {}", e);
            return server_error(RpcError::RequestBroadcastFail, id)
        }

        // Waiting network response
        match self.waiting_for_response().await {
            Ok(resp) => match resp {
                Some(response) => {
                    info!("Key found!");
                    self.state.write().await.map.insert(response.key, response.value.clone());
                    let string = std::str::from_utf8(&response.value).unwrap();
                    JsonResponse::new(json!(string), id).into()
                }
                None => {
                    info!("Did not find key: {}", key);
                    server_error(RpcError::UnknownKey, id).into()
                }
            },
            Err(e) => {
                error!("Failed to query key: {}", e);
                server_error(RpcError::QueryFailed, id).into()
            }
        }
    }

    // Auxilary function to wait for a key response from the P2P network.
    // TODO: if no node holds the key, we shouldn't wait until the request timeout.
    async fn waiting_for_response(&self) -> Result<Option<KeyResponse>> {
        let ex = Arc::new(async_executor::Executor::new());
        let (timeout_s, timeout_r) = async_channel::unbounded::<()>();
        ex.spawn(async move {
            sleep(Duration::from_millis(REQUEST_TIMEOUT).as_secs()).await;
            timeout_s.send(()).await.unwrap_or(());
        })
        .detach();

        loop {
            select! {
                msg =  self.p2p_recv_channel.recv().fuse() => {
                    let response = msg?;
                    return Ok(Some(response))
                },
                _ = self.stop_signal.recv().fuse() => break,
                _ = timeout_r.recv().fuse() => break,
            }
        }
        Ok(None)
    }

    // RPCAPI:
    // Insert key value pair in local map.
    // --> {"jsonrpc": "2.0", "method": "insert", "params": ["key", "value"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "(key, value)", "id": 1}
    async fn insert(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 2 || !params[0].is_string() || !params[1].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let key = params[0].to_string();
        let value = params[1].to_string();

        self.state.write().await.map.insert(key.clone(), value.as_bytes().to_vec());
        // TODO: inform network for the insert/update

        JsonResponse::new(json!((key, value)), id).into()
    }

    // RPCAPI:
    // Returns current local map.
    // --> {"jsonrpc": "2.0", "method": "map", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "map", "id": 1}
    pub async fn map(&self, id: Value, _params: &[Value]) -> JsonResult {
        let map = self.state.read().await.map.clone();
        JsonResponse::new(json!(map), id).into()
    }
}

#[async_trait]
impl RequestHandler for Dhtd {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return JsonError::new(InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        match req.method.as_str() {
            Some("get") => return self.get(req.id, params).await,
            Some("insert") => return self.insert(req.id, params).await,
            Some("map") => return self.map(req.id, params).await,
            Some(_) | None => return JsonError::new(MethodNotFound, None, req.id).into(),
        }
    }
}

// Auxilary function to periodically prun seen messages, based on when they were received.
// This helps us to prevent broadcasting loops.
async fn prune_seen_messages(state: StatePtr) {
    loop {
        sleep(SEEN_DURATION as u64).await;
        debug!("Pruning seen messages");

        let now = Utc::now().timestamp();

        let mut prune = vec![];
        let map = state.read().await.seen.clone();
        for (k, v) in map.iter() {
            if now - v > SEEN_DURATION {
                prune.push(k);
            }
        }

        let mut map = map.clone();
        for i in prune {
            map.remove(i);
        }

        state.write().await.seen = map;
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'_>>) -> Result<()> {
    // We use this handler to block this function after detaching all
    // tasks, and to catch a shutdown signal, where we can clean up and
    // exit gracefully.
    let (signal, shutdown) = async_channel::bounded::<()>(1);
    ctrlc_async::set_async_handler(async move {
        signal.send(()).await.unwrap();
    })
    .unwrap();

    // Initialize daemon state
    let state = State::new().await?;

    // P2P network
    let network_settings = net::Settings {
        inbound: args.p2p_accept,
        outbound_connections: args.slots,
        external_addr: args.p2p_external,
        peers: args.p2p_seed.clone(),
        seeds: args.p2p_seed.clone(),
        ..Default::default()
    };

    let (p2p_send_channel, p2p_recv_channel) = async_channel::unbounded::<KeyResponse>();
    let p2p = net::P2p::new(network_settings).await;
    let registry = p2p.protocol_registry();

    info!("Registering P2P protocols...");
    let _state = state.clone();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let sender = p2p_send_channel.clone();
            let state = _state.clone();
            async move { Protocol::init(channel, sender, state, p2p).await.unwrap() }
        })
        .await;

    // Initialize program state
    let dhtd = Dhtd::new(state.clone(), p2p.clone(), p2p_recv_channel, shutdown.clone()).await?;
    let dhtd = Arc::new(dhtd);

    // Task to periodically clean up daemon seen messages
    ex.spawn(prune_seen_messages(state.clone())).detach();

    // JSON-RPC server
    info!("Starting JSON-RPC server");
    ex.spawn(listen_and_serve(args.rpc_listen, dhtd.clone())).detach();

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
