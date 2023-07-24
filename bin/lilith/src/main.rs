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

use std::{
    collections::{HashMap, HashSet},
    path::Path,
    process::exit,
};

use async_std::{stream::StreamExt, sync::Arc};
use async_trait::async_trait;
use futures::future::join_all;
use log::{debug, error, info, warn};
use semver::Version;
use serde_json::json;
use smol::Executor;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use toml::Value;
use url::Url;

use darkfi::{
    async_daemonize, cli_desc,
    net::{self, connector::Connector, protocol::ProtocolVersion, session::Session, P2p, P2pPtr},
    rpc::{
        jsonrpc::{
            ErrorCode::{InvalidParams, MethodNotFound},
            JsonError, JsonRequest, JsonResponse, JsonResult,
        },
        server::{listen_and_serve, RequestHandler},
    },
    util::{
        async_util::sleep,
        file::{load_file, save_file},
        path::{expand_path, get_config_path},
    },
    Result,
};

const CONFIG_FILE: &str = "lilith_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../lilith_config.toml");

#[derive(Clone, Debug, serde::Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "lilith", about = cli_desc!())]
struct Args {
    #[structopt(long, default_value = "tcp://127.0.0.1:18927")]
    /// JSON-RPC listen URL
    pub rpc_listen: Url,

    #[structopt(long)]
    /// Accept addresses (URL without port)
    pub accept_addrs: Vec<Url>,

    #[structopt(short, long)]
    /// Configuration file to use
    pub config: Option<String>,

    #[structopt(long, default_value = "~/.config/darkfi/lilith_hosts.tsv")]
    /// Hosts .tsv file to use
    pub hosts_file: String,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    pub verbose: u8,
}

/// Struct representing a spawned P2P network
struct Spawn {
    /// String identifier,
    pub name: String,
    /// P2P pointer
    pub p2p: P2pPtr,
}

impl Spawn {
    async fn addresses(&self) -> Vec<String> {
        self.p2p.hosts().load_all().await.iter().map(|addr| addr.to_string()).collect()
    }

    async fn info(&self) -> serde_json::Value {
        let mut addr_vec = vec![];
        for addr in &self.p2p.settings().inbound_addrs {
            addr_vec.push(addr.as_ref().to_string());
        }

        json!({
            "name": self.name.clone(),
            "urls": addr_vec,
            "hosts": self.addresses().await,
        })
    }
}

/// Defines the network-specific settings
#[derive(Clone)]
struct NetInfo {
    /// Specific port the network will use
    pub port: u16,
    /// Other seeds to connect to
    pub seeds: Vec<Url>,
    /// Manual peers to connect to
    pub peers: Vec<Url>,
    /// Supported network version
    pub version: Version,
    /// Enable localnet hosts
    pub localnet: bool,
}

/// Struct representing the daemon
struct Lilith {
    /// Spawned networks
    pub networks: Vec<Spawn>,
}

impl Lilith {
    /// Internal task to run a periodic purge of unreachable hosts
    /// for a specific P2P network.
    async fn periodic_purge(name: String, p2p: P2pPtr, ex: Arc<Executor<'_>>) {
        info!("Starting periodic host purge task for \"{}\"", name);
        loop {
            // We'll pick up to 10 hosts every minute and try to connect to
            // them. If we can't reach them, we'll remove them from our set.
            sleep(60).await;
            debug!("[{}] Picking random hosts from db", name);
            let lottery_winners = p2p.clone().hosts().get_n_random(10).await;
            let win_str: Vec<&str> = lottery_winners.iter().map(|x| x.as_str()).collect();
            debug!("[{}] Got: {:?}", name, win_str);

            let mut tasks = vec![];

            for host in &lottery_winners {
                let p2p_ = p2p.clone();
                let ex_ = ex.clone();
                tasks.push(async move {
                    let session_out = p2p_.session_outbound().await;
                    let session_weak = Arc::downgrade(&p2p_.session_outbound().await);

                    let connector = Connector::new(p2p_.settings(), Arc::new(session_weak));
                    debug!("Connecting to {}", host);
                    match connector.connect(host).await {
                        Ok((_url, channel)) => {
                            debug!("Connected successfully!");
                            let proto_ver = ProtocolVersion::new(
                                channel.clone(),
                                p2p_.settings().clone(),
                                p2p_.hosts().clone(),
                            )
                            .await;

                            let handshake_task = session_out.perform_handshake_protocols(
                                proto_ver,
                                channel.clone(),
                                ex_.clone(),
                            );

                            channel.clone().start(ex_.clone());

                            match handshake_task.await {
                                Ok(()) => {
                                    debug!("Handshake success! Stopping channel.");
                                    channel.stop().await;
                                }
                                Err(e) => {
                                    debug!("Handshake failure! {}", e);
                                }
                            }
                        }

                        Err(e) => {
                            debug!("Failed to connect to {}, removing from set ({})", host, e);
                            // Remove from hosts set
                            p2p_.hosts().remove(host).await;
                        }
                    }
                });
            }

            join_all(tasks).await;
        }
    }

    // RPCAPI:
    // Replies to a ping method.
    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result", "pong", "id: 42}
    async fn pong(&self, id: serde_json::Value, _params: &[serde_json::Value]) -> JsonResult {
        JsonResponse::new(json!("pong"), id).into()
    }

    // RPCAPI:
    // Returns all spawned networks names with their node addresses.
    // --> {"jsonrpc": "2.0", "method": "spawns", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": {"spawns": spawns_info}, "id": 42}
    async fn spawns(&self, id: serde_json::Value, _params: &[serde_json::Value]) -> JsonResult {
        let mut spawns = vec![];
        for spawn in &self.networks {
            spawns.push(spawn.info().await);
        }

        let json = json!({ "spawns": spawns });

        JsonResponse::new(json, id).into()
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

/// Attempt to read existing hosts tsv
fn load_hosts(path: &Path, networks: &[&str]) -> HashMap<String, HashSet<Url>> {
    let mut saved_hosts = HashMap::new();

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
                None => HashSet::new(),
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

async fn save_hosts(path: &Path, networks: &[Spawn]) {
    let mut tsv = String::new();

    for spawn in networks {
        for host in spawn.p2p.hosts().load_all().await {
            tsv.push_str(&format!("{}\t{}\n", spawn.name, host.as_str()));
        }
    }

    if !tsv.eq("") {
        info!("Saving current hosts of spawned networks to: {:?}", path);
        if let Err(e) = save_file(path, &tsv) {
            error!("Failed saving hosts: {}", e);
        }
    }
}

/// Parse a TOML string for any configured network and return a map containing
/// said configurations.
fn parse_configured_networks(data: &str) -> Result<HashMap<String, NetInfo>> {
    let mut ret = HashMap::new();

    if let Value::Table(map) = toml::from_str(data)? {
        if map.contains_key("network") && map["network"].is_table() {
            for net in map["network"].as_table().unwrap() {
                info!("Found configuration for network: {}", net.0);
                let table = net.1.as_table().unwrap();
                if !table.contains_key("port") {
                    warn!("Network port is mandatory, skipping network.");
                    continue
                }

                let name = net.0.to_string();
                let port = table["port"].as_integer().unwrap().try_into().unwrap();

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

                let net_info = NetInfo { port, seeds, peers, version, localnet };
                ret.insert(name, net_info);
            }
        }
    }

    Ok(ret)
}

async fn spawn_net(
    name: String,
    info: &NetInfo,
    accept_addrs: &[Url],
    saved_hosts: &HashSet<Url>,
    ex: Arc<Executor<'_>>,
) -> Result<Spawn> {
    let mut listen_urls = vec![];

    // Configure listen addrs for this network
    for url in accept_addrs {
        let mut url = url.clone();
        url.set_port(Some(info.port))?;
        listen_urls.push(url);
    }

    // P2P network settings
    let settings = net::Settings {
        inbound_addrs: listen_urls.clone(),
        seeds: info.seeds.clone(),
        peers: info.peers.clone(),
        outbound_connections: 0,
        outbound_connect_timeout: 30,
        app_version: info.version.clone(),
        localnet: info.localnet,
        allowed_transports: vec![
            "tcp".to_string(),
            "tcp+tls".to_string(),
            "tor".to_string(),
            "tor+tls".to_string(),
            "nym".to_string(),
            "nym+tls".to_string(),
        ],
        ..Default::default()
    };

    // Create P2P instance
    let p2p = P2p::new(settings).await;

    // Fill db with cached hosts
    let hosts: Vec<Url> = saved_hosts.iter().cloned().collect();
    p2p.hosts().store(&hosts).await;

    let addrs_str: Vec<&str> = listen_urls.iter().map(|x| x.as_str()).collect();
    info!("Starting seed network node for \"{}\" on {:?}", name, addrs_str);
    p2p.clone().start(ex.clone()).await?;
    let name_ = name.clone();
    let p2p_ = p2p.clone();
    let ex_ = ex.clone();
    ex.spawn(async move {
        if let Err(e) = p2p_.run(ex_).await {
            error!("Failed starting P2P network seed for \"{}\": {}", name_, e);
        }
    })
    .detach();

    let spawn = Spawn { name, p2p };
    Ok(spawn)
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'_>>) -> Result<()> {
    // Pick up network settings from the TOML config
    let cfg_path = get_config_path(args.config, CONFIG_FILE)?;
    let toml_contents = std::fs::read_to_string(cfg_path)?;
    let configured_nets = parse_configured_networks(&toml_contents)?;

    if configured_nets.is_empty() {
        error!("No networks are enabled in config");
        exit(1);
    }

    // Retrieve any saved hosts for configured networks
    let net_names: Vec<&str> = configured_nets.keys().map(|x| x.as_str()).collect();
    let saved_hosts = load_hosts(&expand_path(&args.hosts_file)?, &net_names);

    // Spawn configured networks
    let mut networks = vec![];
    for (name, info) in &configured_nets {
        // TODO: Here we could actually differentiate between network versions
        // e.g. p2p_v3, p2p_v4, etc. Therefore we can spawn multiple networks
        // and they would all be version-checked, so we avoid mismatches when
        // seeding peers.
        match spawn_net(
            name.to_string(),
            info,
            &args.accept_addrs,
            saved_hosts.get(name).unwrap_or(&HashSet::new()),
            ex.clone(),
        )
        .await
        {
            Ok(spawn) => networks.push(spawn),
            Err(e) => {
                error!("Failed to start P2P network seed for \"{}\": {}", name, e);
                exit(1);
            }
        }
    }

    // Set up main daemon and background tasks
    let lilith = Arc::new(Lilith { networks });
    for network in &lilith.networks {
        let name = network.name.clone();
        ex.spawn(Lilith::periodic_purge(name, network.p2p.clone(), ex.clone())).detach();
    }

    // JSON-RPC server
    info!("Starting JSON-RPC server on {}", args.rpc_listen);
    let _ex = ex.clone();
    ex.spawn(listen_and_serve(args.rpc_listen, lilith.clone(), _ex)).detach();

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new()?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    // Save in-memory hosts to tsv file
    save_hosts(&expand_path(&args.hosts_file)?, &lilith.networks).await;

    // Cleanly stop p2p networks
    for spawn in &lilith.networks {
        info!("Stopping \"{}\" P2P", spawn.name);
        spawn.p2p.stop().await;
    }

    info!("Bye!");
    Ok(())
}
