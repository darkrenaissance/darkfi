use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use futures_lite::future;
use log::{error, info};
use serde_json::{json, Value};
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::{
    async_daemonize, net,
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
        expand_path,
        path::get_config_path,
    },
    Result,
};

mod config;
use config::{parse_configured_networks, Args, NetInfo};

const CONFIG_FILE: &str = "lilith_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../lilith_config.toml");

/// Struct representing a spawned p2p network.
pub struct Spawn {
    pub name: String,
    pub p2p: P2pPtr,
}

impl Spawn {
    async fn addresses(&self) -> Vec<String> {
        self.p2p.hosts().load_all().await.iter().map(|addr| addr.to_string()).collect()
    }

    pub async fn info(&self) -> serde_json::Value {
        // Building addr_vec string
        let mut addr_vec = vec![];
        for addr in &self.p2p.settings().inbound {
            addr_vec.push(addr.as_ref().to_string());
        }

        json!({
            "name": self.name.clone(),
            "urls": addr_vec,
            "hosts": self.addresses().await,
        })
    }
}

/// Struct representing the daemon.
pub struct Lilith {
    /// Configured urls
    urls: Vec<Url>,
    /// Spawned networks
    spawns: Vec<Spawn>,
}

impl Lilith {
    // RPCAPI:
    // Returns all spawned networks names with their node addresses.
    // --> {"jsonrpc": "2.0", "method": "spawns", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "{spawns}", "id": 42}
    async fn spawns(&self, id: Value, _params: &[Value]) -> JsonResult {
        // Building urls string
        let mut urls_vec = vec![];
        for url in &self.urls {
            urls_vec.push(url.as_ref().to_string());
        }

        // Gathering spawns info
        let mut spawns = vec![];
        for spawn in &self.spawns {
            spawns.push(spawn.info().await);
        }

        // Generating json
        let json = json!({
            "urls": urls_vec,
            "spawns": spawns,
        });
        JsonResponse::new(json, id).into()
    }

    // RPCAPI:
    // Replies to a ping method.
    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 42}
    async fn pong(&self, id: Value, _params: &[Value]) -> JsonResult {
        JsonResponse::new(json!("pong"), id).into()
    }
}

#[async_trait]
impl RequestHandler for Lilith {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return JsonError::new(InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        match req.method.as_str() {
            Some("spawns") => return self.spawns(req.id, params).await,
            Some("ping") => return self.pong(req.id, params).await,
            Some(_) | None => return JsonError::new(MethodNotFound, None, req.id).into(),
        }
    }
}

async fn spawn_network(
    name: &str,
    info: NetInfo,
    urls: Vec<Url>,
    ex: Arc<Executor<'_>>,
) -> Result<Spawn> {
    let mut full_urls = Vec::new();
    for url in &urls {
        let mut url = url.clone();
        url.set_port(Some(info.port))?;
        full_urls.push(url);
    }
    let network_settings = net::Settings {
        inbound: full_urls.clone(),
        seeds: info.seeds,
        peers: info.peers,
        outbound_connections: 0,
        localnet: info.localnet,
        ..Default::default()
    };

    let p2p = net::P2p::new(network_settings).await;

    // Building ext_addr_vec string
    let mut urls_vec = vec![];
    for url in &full_urls {
        urls_vec.push(url.as_ref().to_string());
    }
    info!("Starting seed network node for {} at: {:?}", name, urls_vec);
    p2p.clone().start(ex.clone()).await?;
    let _ex = ex.clone();
    let _p2p = p2p.clone();
    ex.spawn(async move {
        if let Err(e) = _p2p.run(_ex).await {
            error!("Failed starting P2P network seed: {}", e);
        }
    })
    .detach();

    let spawn = Spawn { name: name.to_string(), p2p };

    Ok(spawn)
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

    // Pick up network settings from the TOML configuration
    let cfg_path = get_config_path(args.config, CONFIG_FILE)?;
    let toml_contents = std::fs::read_to_string(cfg_path)?;
    let configured_nets = parse_configured_networks(&toml_contents)?;

    // Verify any daemon network is enabled
    if configured_nets.is_empty() {
        info!("No daemon network is enabled!");
        return Ok(())
    }

    // Setting urls
    let mut urls = args.urls.clone();
    if urls.is_empty() {
        info!("Urls are not provided, will use: tcp://127.0.0.1");
        let url = Url::parse("tcp://127.0.0.1")?;
        urls.push(url);
    }

    // Spawn configured networks
    let mut spawns = vec![];
    for (name, info) in &configured_nets {
        match spawn_network(name, info.clone(), urls.clone(), ex.clone()).await {
            Ok(spawn) => spawns.push(spawn),
            Err(e) => error!("Failed starting {} P2P network seed: {}", name, e),
        }
    }

    let lilith = Lilith { urls, spawns };
    let lilith = Arc::new(lilith);

    // JSON-RPC server
    info!("Starting JSON-RPC server");
    ex.spawn(listen_and_serve(args.rpc_listen, lilith)).detach();

    // Wait for SIGINT
    shutdown.recv().await?;
    print!("\r");
    info!("Caught termination signal, cleaning up and exiting...");

    Ok(())
}
