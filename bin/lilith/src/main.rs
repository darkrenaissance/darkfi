/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use std::path::Path;

use async_std::sync::Arc;
use async_trait::async_trait;
use fxhash::{FxHashMap, FxHashSet};
use log::{error, info, warn};
use serde_json::{json, Value};
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::{
    async_daemonize, net,
    net::P2pPtr,
    rpc::{
        jsonrpc::{
            ErrorCode::{InvalidParams, MethodNotFound},
            JsonError, JsonNotification, JsonRequest, JsonResponse, JsonResult, JsonSubscriber,
        },
        server::{listen_and_serve, RequestHandler},
    },
    system::{Subscriber, SubscriberPtr},
    util::{
        async_util::sleep,
        file::{load_file, save_file},
        path::{expand_path, get_config_path},
    },
    Result,
};

mod config;
use config::{parse_configured_networks, Args, NetInfo};

const CONFIG_FILE: &str = "lilith_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../lilith_config.toml");

/// Struct representing a spawned p2p network.
struct Spawn {
    name: String,
    p2p: P2pPtr,
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
struct Lilith {
    /// Configured urls
    urls: Vec<Url>,
    /// Spawned networks
    spawns: Vec<Spawn>,
    // TODO: Subscriber should come from ValidatorState or something
    subscriber: SubscriberPtr<JsonNotification>,
}

impl Lilith {
    async fn spawns_hosts(&self) -> FxHashMap<String, Vec<String>> {
        // Building urls string
        let mut spawns = FxHashMap::default();
        for spawn in &self.spawns {
            spawns.insert(spawn.name.clone(), spawn.addresses().await);
        }

        spawns
    }

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

    // RPCAPI:
    // Create a new subscriber for new blocks to notify connected peer.
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.notify_blocks", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn blockchain_notify_blocks(&self, id: Value, params: &[Value]) -> JsonResult {
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        JsonSubscriber::new(id, self.subscriber.clone()).into()
    }
}

// TODO: remove this
async fn simulate_blocks(subscriber: SubscriberPtr<JsonNotification>) {
    // Notifications simulation
    let message =
        JsonNotification::new("blockchain.notify_blocks", Value::from("New Block created!"));
    loop {
        subscriber.notify(message.clone()).await;
        sleep(10).await;
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
            Some("blockchain.notify_blocks") => {
                return self.blockchain_notify_blocks(req.id, params).await
            }
            Some(_) | None => return JsonError::new(MethodNotFound, None, req.id).into(),
        }
    }
}

async fn spawn_network(
    name: &str,
    info: NetInfo,
    urls: Vec<Url>,
    saved_hosts: Option<&FxHashSet<Url>>,
    ex: Arc<smol::Executor<'_>>,
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
        channel_log: info.channel_log,
        app_version: None,
        ..Default::default()
    };

    let p2p = net::P2p::new(network_settings).await;

    // Setting saved hosts
    match saved_hosts {
        Some(hosts) => {
            // Converting hashet to vec
            let mut vec = vec![];
            for url in hosts {
                vec.push(url.clone());
            }
            p2p.hosts().store(vec).await;
        }
        None => info!("No saved hosts found for {}", name),
    }

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

/// Retrieve saved hosts for provided networks
fn load_hosts(path: &Path, networks: &[&str]) -> FxHashMap<String, FxHashSet<Url>> {
    let mut saved_hosts = FxHashMap::default();
    info!("Retrieving saved hosts from: {:?}", path);
    let contents = load_file(path);
    if let Err(e) = contents {
        warn!("Failed retrieving saved hosts: {}", e);
        return saved_hosts
    }

    for line in contents.unwrap().lines() {
        let data: Vec<&str> = line.split('\t').collect();
        if networks.contains(&data[0]) {
            let mut hosts = match saved_hosts.get(data[0]) {
                Some(hosts) => hosts.clone(),
                None => FxHashSet::default(),
            };
            let url = match Url::parse(data[1]) {
                Ok(u) => u,
                Err(e) => {
                    warn!("Skipping malformed url: {} ({})", data[1], e);
                    continue
                }
            };
            hosts.insert(url);
            saved_hosts.insert(data[0].to_string(), hosts);
        }
    }

    saved_hosts
}

/// Save spawns current hosts
fn save_hosts(path: &Path, spawns: FxHashMap<String, Vec<String>>) {
    let mut string = "".to_string();
    for (name, urls) in spawns {
        for url in urls {
            string.push_str(&name);
            string.push('\t');
            string.push_str(&url);
            string.push('\n');
        }
    }

    if !string.eq("") {
        info!("Saving current hosts of spawnned networks to: {:?}", path);
        if let Err(e) = save_file(path, &string) {
            error!("Failed saving hosts: {}", e);
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

    // Retrieve saved hosts for configured networks
    let full_path = expand_path(&args.hosts_file)?;
    let nets: Vec<&str> = configured_nets.keys().map(|x| x.as_str()).collect();
    let saved_hosts = load_hosts(&full_path, &nets);

    // Spawn configured networks
    let mut spawns = vec![];
    for (name, info) in &configured_nets {
        match spawn_network(name, info.clone(), urls.clone(), saved_hosts.get(name), ex.clone())
            .await
        {
            Ok(spawn) => spawns.push(spawn),
            Err(e) => error!("Failed starting {} P2P network seed: {}", name, e),
        }
    }

    // TODO: Subscriber should come from ValidatorState or something
    let subscriber: SubscriberPtr<JsonNotification> = Subscriber::new();

    let lilith = Lilith { urls, spawns, subscriber: subscriber.clone() };
    let lilith = Arc::new(lilith);

    // JSON-RPC server
    info!("Starting JSON-RPC server");
    ex.spawn(listen_and_serve(args.rpc_listen, lilith.clone())).detach();

    // JSON-RPC notifications simulation
    let _ex = ex.clone();
    ex.spawn(simulate_blocks(subscriber)).detach();

    // Wait for SIGINT
    shutdown.recv().await?;
    print!("\r");
    info!("Caught termination signal, cleaning up and exiting...");

    // Save spawns current hosts
    save_hosts(&full_path, lilith.spawns_hosts().await);

    Ok(())
}
