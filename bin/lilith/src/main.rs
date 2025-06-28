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
    process::exit,
    sync::Arc,
    time::UNIX_EPOCH,
};

use async_trait::async_trait;
use log::{debug, error, info, warn};
use semver::Version;
use smol::{
    lock::{Mutex, MutexGuard},
    stream::StreamExt,
    Executor,
};
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use tinyjson::JsonValue;
use toml::Value;
use url::Url;

use darkfi::{
    async_daemonize, cli_desc,
    net::{self, hosts::HostColor, settings::BanPolicy, P2p, P2pPtr},
    rpc::{
        jsonrpc::*,
        server::{listen_and_serve, RequestHandler},
        settings::{RpcSettings, RpcSettingsOpt},
    },
    system::{sleep, StoppableTask, StoppableTaskPtr},
    util::path::get_config_path,
    Error, Result,
};

const CONFIG_FILE: &str = "lilith_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../lilith_config.toml");

#[derive(Clone, Debug, serde::Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "lilith", about = cli_desc!())]
struct Args {
    #[structopt(flatten)]
    /// JSON-RPC settings
    rpc: RpcSettingsOpt,

    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[structopt(long, default_value = "120")]
    /// Interval after which to check whitelist peers
    whitelist_refinery_interval: u64,
}

/// Struct representing a spawned P2P network
struct Spawn {
    /// String identifier,
    pub name: String,
    /// P2P pointer
    pub p2p: P2pPtr,
}

impl Spawn {
    async fn get_whitelist(&self) -> Vec<JsonValue> {
        self.p2p
            .hosts()
            .container
            .fetch_all(HostColor::White)
            .iter()
            .map(|(addr, _url)| JsonValue::String(addr.to_string()))
            .collect()
    }

    async fn get_greylist(&self) -> Vec<JsonValue> {
        self.p2p
            .hosts()
            .container
            .fetch_all(HostColor::Grey)
            .iter()
            .map(|(addr, _url)| JsonValue::String(addr.to_string()))
            .collect()
    }

    async fn get_goldlist(&self) -> Vec<JsonValue> {
        self.p2p
            .hosts()
            .container
            .fetch_all(HostColor::Gold)
            .iter()
            .map(|(addr, _url)| JsonValue::String(addr.to_string()))
            .collect()
    }

    async fn info(&self) -> JsonValue {
        let mut addr_vec = vec![];
        for addr in &self.p2p.settings().read().await.inbound_addrs {
            addr_vec.push(JsonValue::String(addr.as_ref().to_string()));
        }

        JsonValue::Object(HashMap::from([
            ("name".to_string(), JsonValue::String(self.name.clone())),
            ("urls".to_string(), JsonValue::Array(addr_vec)),
            ("whitelist".to_string(), JsonValue::Array(self.get_whitelist().await)),
            ("greylist".to_string(), JsonValue::Array(self.get_greylist().await)),
            ("goldlist".to_string(), JsonValue::Array(self.get_goldlist().await)),
        ]))
    }
}

/// Defines the network-specific settings
#[derive(Clone)]
struct NetInfo {
    /// Accept addresses the network will use
    pub accept_addrs: Vec<Url>,
    /// Other seeds to connect to
    pub seeds: Vec<Url>,
    /// Manual peers to connect to
    pub peers: Vec<Url>,
    /// Supported network version
    pub version: Version,
    /// Enable localnet hosts
    pub localnet: bool,
    /// Path to P2P datastore
    pub datastore: String,
    /// Path to hostlist
    pub hostlist: String,
}

/// Struct representing the daemon
struct Lilith {
    /// Spawned networks
    pub networks: Vec<Spawn>,
    /// JSON-RPC connection tracker
    pub rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

impl Lilith {
    /// Since `Lilith` does not make outbound connections, if a peer is
    /// upgraded to whitelist it will remain on the whitelist even if the
    /// give peer is no longer online.
    ///
    /// To protect `Lilith` from sharing potentially offline nodes,
    /// `whitelist_refinery` periodically ping nodes on the whitelist. If they
    /// are reachable, we update their last seen field. Otherwise, we downgrade
    /// them to the greylist.
    ///
    /// Note: if `Lilith` loses connectivity this method will delete peers from
    /// the whitelist, meaning `Lilith` will need to rebuild its hostlist when
    /// it comes back online.
    async fn whitelist_refinery(
        network_name: String,
        p2p: P2pPtr,
        refinery_interval: u64,
    ) -> Result<()> {
        debug!(target: "net::refinery::whitelist_refinery", "Starting whitelist refinery for \"{network_name}\"");

        let hosts = p2p.hosts();

        loop {
            sleep(refinery_interval).await;

            match hosts.container.fetch_last(HostColor::White) {
                Some(entry) => {
                    let url = &entry.0;
                    let last_seen = &entry.1;

                    if !hosts.refinable(url.clone()) {
                        debug!(target: "net::refinery::whitelist_refinery", "Addr={} not available!",
                       url.clone());

                        continue
                    }

                    if !p2p.session_refine().handshake_node(url.clone(), p2p.clone()).await {
                        debug!(target: "net::refinery:::whitelist_refinery",
                       "Host {url} is not responsive. Downgrading from whitelist");

                        hosts.greylist_host(url, *last_seen)?;

                        continue
                    }

                    debug!(target: "net::refinery::whitelist_refinery",
                   "Peer {url} is responsive. Updating last_seen");

                    // This node is active. Update the last seen field.
                    let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();

                    hosts.whitelist_host(url, last_seen)?;
                }
                None => {
                    debug!(target: "net::refinery::whitelist_refinery",
                              "Whitelist is empty! Cannot start refinery process");

                    continue
                }
            }
        }
    }
    // RPCAPI:
    // Returns all spawned networks names with their node addresses.
    // --> {"jsonrpc": "2.0", "method": "spawns", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": {"spawns": spawns_info}, "id": 42}
    async fn spawns(&self, id: u16, _params: JsonValue) -> JsonResult {
        let mut spawns = vec![];
        for spawn in &self.networks {
            spawns.push(spawn.info().await);
        }

        let json =
            JsonValue::Object(HashMap::from([("spawns".to_string(), JsonValue::Array(spawns))]));

        JsonResponse::new(json, id).into()
    }
}

#[async_trait]
impl RequestHandler<()> for Lilith {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        return match req.method.as_str() {
            "ping" => self.pong(req.id, req.params).await,
            "spawns" => self.spawns(req.id, req.params).await,
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

/// Parse a TOML string for any configured network and return a map containing
/// said configurations.
fn parse_configured_networks(data: &str) -> Result<HashMap<String, NetInfo>> {
    let mut ret = HashMap::new();

    if let Value::Table(map) = toml::from_str(data)? {
        if map.contains_key("network") && map["network"].is_table() {
            for net in map["network"].as_table().unwrap() {
                info!(target: "lilith", "Found configuration for network: {}", net.0);
                let table = net.1.as_table().unwrap();
                if !table.contains_key("accept_addrs") {
                    warn!(target: "lilith", "Network accept addrs are mandatory, skipping network.");
                    continue
                }

                if !table.contains_key("hostlist") {
                    error!(target: "lilith", "Hostlist path is mandatory! Configure and try again.");
                    exit(1)
                }

                let name = net.0.to_string();
                let accept_addrs: Vec<Url> = table["accept_addrs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| Url::parse(x.as_str().unwrap()).unwrap())
                    .collect();

                let mut seeds = vec![];
                if table.contains_key("seeds") {
                    if let Some(s) = table["seeds"].as_array() {
                        for seed in s {
                            if let Some(u) = seed.as_str() {
                                if let Ok(url) = Url::parse(u) {
                                    seeds.push(url);
                                }
                            }
                        }
                    }
                }

                let mut peers = vec![];
                if table.contains_key("peers") {
                    if let Some(p) = table["peers"].as_array() {
                        for peer in p {
                            if let Some(u) = peer.as_str() {
                                if let Ok(url) = Url::parse(u) {
                                    peers.push(url);
                                }
                            }
                        }
                    }
                }

                let localnet = if table.contains_key("localnet") {
                    table["localnet"].as_bool().unwrap()
                } else {
                    false
                };

                let version = if table.contains_key("version") {
                    semver::Version::parse(table["version"].as_str().unwrap())?
                } else {
                    semver::Version::parse(option_env!("CARGO_PKG_VERSION").unwrap_or("0.0.0"))?
                };

                let datastore: String = table["datastore"].as_str().unwrap().to_string();

                let hostlist: String = table["hostlist"].as_str().unwrap().to_string();

                let net_info =
                    NetInfo { accept_addrs, seeds, peers, version, localnet, datastore, hostlist };
                ret.insert(name, net_info);
            }
        }
    }

    Ok(ret)
}

async fn spawn_net(name: String, info: &NetInfo, ex: Arc<Executor<'static>>) -> Result<Spawn> {
    let mut listen_urls = vec![];

    // Configure listen addrs for this network
    for url in &info.accept_addrs {
        listen_urls.push(url.clone());
    }

    // P2P network settings
    let settings = net::Settings {
        inbound_addrs: listen_urls.clone(),
        seeds: info.seeds.clone(),
        peers: info.peers.clone(),
        outbound_connections: 0,
        outbound_connect_timeout: 30,
        inbound_connections: 512,
        app_version: info.version.clone(),
        localnet: info.localnet,
        p2p_datastore: Some(info.datastore.clone()),
        hostlist: Some(info.hostlist.clone()),
        allowed_transports: vec![
            "tcp".to_string(),
            "tcp+tls".to_string(),
            "tor".to_string(),
            "tor+tls".to_string(),
            "nym".to_string(),
            "nym+tls".to_string(),
            "i2p".to_string(),
            "i2p+tls".to_string(),
        ],
        ban_policy: BanPolicy::Relaxed,
        ..Default::default()
    };

    // Create P2P instance
    let p2p = P2p::new(settings, ex.clone()).await?;

    let addrs_str: Vec<&str> = listen_urls.iter().map(|x| x.as_str()).collect();
    info!(target: "lilith", "Starting seed network node for \"{name}\" on {addrs_str:?}");
    p2p.clone().start().await?;

    let spawn = Spawn { name, p2p };
    Ok(spawn)
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
    // Pick up network settings from the TOML config
    let cfg_path = get_config_path(args.config, CONFIG_FILE)?;
    let toml_contents = std::fs::read_to_string(cfg_path)?;
    let configured_nets = parse_configured_networks(&toml_contents)?;

    if configured_nets.is_empty() {
        error!(target: "lilith", "No networks are enabled in config");
        exit(1);
    }

    // Spawn configured networks
    let mut networks = vec![];
    for (name, info) in &configured_nets {
        match spawn_net(name.to_string(), info, ex.clone()).await {
            Ok(spawn) => networks.push(spawn),
            Err(e) => {
                error!(target: "lilith", "Failed to start P2P network seed for \"{name}\": {e}");
                exit(1);
            }
        }
    }

    // Set up main daemon and background refinery_tasks
    let lilith = Arc::new(Lilith { networks, rpc_connections: Mutex::new(HashSet::new()) });
    let mut refinery_tasks = HashMap::new();
    for network in &lilith.networks {
        let name = network.name.clone();
        let task = StoppableTask::new();
        task.clone().start(
            Lilith::whitelist_refinery(name.clone(), network.p2p.clone(), args.whitelist_refinery_interval),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "lilith", "Failed starting refinery task for \"{name}\": {e}"),
                }
            },
            Error::DetachedTaskStopped,
            ex.clone(),
        );
        refinery_tasks.insert(network.name.clone(), task);
    }

    // JSON-RPC server
    let rpc_settings: RpcSettings = args.rpc.into();
    info!(target: "lilith", "Starting JSON-RPC server on {}", rpc_settings.listen);
    let lilith_ = lilith.clone();
    let rpc_task = StoppableTask::new();
    rpc_task.clone().start(
        listen_and_serve(rpc_settings, lilith.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => lilith_.stop_connections().await,
                Err(e) => error!(target: "lilith", "Failed starting JSON-RPC server: {e}"),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "lilith", "Caught termination signal, cleaning up and exiting...");

    info!(target: "lilith", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    // Cleanly stop p2p networks
    for spawn in &lilith.networks {
        info!(target: "lilith", "Stopping \"{}\" task", spawn.name);
        refinery_tasks.get(&spawn.name).unwrap().stop().await;
        info!(target: "lilith", "Stopping \"{}\" P2P", spawn.name);
        spawn.p2p.stop().await;
    }

    info!(target: "lilith", "Bye!");
    Ok(())
}
