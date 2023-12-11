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
    collections::{HashMap, HashSet, VecDeque},
    path::Path,
    process::exit,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use async_trait::async_trait;
use futures::future::join_all;
use log::{debug, error, info, warn};
use semver::Version;
use smol::{
    lock::{Mutex, MutexGuard, RwLock},
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
    net::{self, connector::Connector, protocol::ProtocolVersion, session::Session, P2p, P2pPtr},
    rpc::{
        jsonrpc::*,
        server::{listen_and_serve, RequestHandler},
    },
    system::{sleep, StoppableTask, StoppableTaskPtr},
    util::{
        file::{load_file, save_file},
        path::{expand_path, get_config_path},
    },
    Error, Result,
};

const CONFIG_FILE: &str = "lilith_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../lilith_config.toml");

/// Period in which the peer purge happens (in seconds)
const CLEANSE_PERIOD: u64 = 60;
/// Amount of hosts to try each purge iteration
const PROBE_HOSTS_N: u32 = 10;

#[derive(Clone, Debug, serde::Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "lilith", about = cli_desc!())]
struct Args {
    #[structopt(long, default_value = "tcp://127.0.0.1:18927")]
    /// JSON-RPC listen URL
    pub rpc_listen: Url,

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
    async fn addresses(&self) -> Vec<JsonValue> {
        self.p2p
            .hosts()
            .whitelist_fetch_all()
            .await
            .iter()
            .map(|(addr, url)| JsonValue::String(addr.to_string()))
            .collect()
    }

    async fn info(&self) -> JsonValue {
        let mut addr_vec = vec![];
        for addr in &self.p2p.settings().inbound_addrs {
            addr_vec.push(JsonValue::String(addr.as_ref().to_string()));
        }

        JsonValue::Object(HashMap::from([
            ("name".to_string(), JsonValue::String(self.name.clone())),
            ("urls".to_string(), JsonValue::Array(addr_vec)),
            ("hosts".to_string(), JsonValue::Array(self.addresses().await)),
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
}

/// Struct representing the daemon
struct Lilith {
    /// Spawned networks
    pub networks: Vec<Spawn>,
    /// JSON-RPC connection tracker
    pub rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

impl Lilith {
    async fn refresh_whitelist(name: String, p2p: P2pPtr, ex: Arc<Executor<'_>>) -> Result<()> {
        info!(target: "lilith", "Starting periodic host cleanse task for \"{}\"", name);

        // Initialize a growable ring buffer(VecDeque) to store known hosts
        let ring_buffer = Arc::new(RwLock::new(VecDeque::<Url>::new()));
        loop {
            // Wait for next purge period
            sleep(CLEANSE_PERIOD).await;
            debug!(target: "lilith", "[{}] The Cleanse has started...", name);

            // Check if new hosts exist and add them to the end of the ring buffer
            let mut lock = ring_buffer.write().await;
            let hosts = p2p.clone().hosts().whitelist_fetch_all().await;
            if hosts.len() != lock.len() {
                // Since hosts are stored in a HashSet we have to check all of them
                for (addr, _last_seen) in hosts {
                    if !lock.contains(&addr) {
                        lock.push_back(addr);
                    }
                }
            }

            // Pick first up to PROBE_HOSTS_N hosts from the ring buffer
            let mut cleansers = vec![];
            let mut index = 0;
            while index <= PROBE_HOSTS_N {
                match lock.pop_front() {
                    Some(host) => cleansers.push(host),
                    None => break,
                };
                index += 1;
            }

            // Try to connect to them. If we establish a connection, update the last_seen() field.
            let cleansers_str: Vec<&str> = cleansers.iter().map(|x| x.as_str()).collect();
            debug!(target: "lilith", "[{}] Got: {:?}", name, cleansers_str);

            let mut tasks = vec![];

            for host in &cleansers {
                let p2p_ = p2p.clone();
                let hosts = p2p_.hosts();
                let ex_ = ex.clone();
                let ring_buffer_ = ring_buffer.clone();

                tasks.push(async move {
                    let mut whitelist = hosts.whitelist.write().await;

                    let session_out = p2p_.session_outbound();
                    let session_weak = Arc::downgrade(&session_out);

                    let connector = Connector::new(p2p_.settings().clone(), session_weak);
                    debug!(target: "lilith", "Connecting to {}", host);
                    match connector.connect(host).await {
                        Ok((_url, channel)) => {
                            debug!(target: "lilith", "Connected successfully!");
                            let proto_ver = ProtocolVersion::new(channel.clone(), p2p_.settings().clone()).await;

                            let handshake_task = session_out.perform_handshake_protocols(
                                proto_ver,
                                channel.clone(),
                                ex_.clone(),
                            );

                            channel.clone().start(ex_.clone());

                            match handshake_task.await {
                                Ok(()) => {
                                    debug!(target: "lilith", "Handshake success! Stopping channel.");
                                    channel.stop().await;

                                    // Peer is responsive. Update last_seen and add it to the whitelist.
                                    let last_seen =
                                        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();

                                    // Remove oldest element if the whitelist reaches max size.
                                    if whitelist.len() == 1000 {
                                        // Last element in vector should have the oldest timestamp.
                                        // This should never crash as only returns None when whitelist len() == 0.
                                        let entry = whitelist.pop().unwrap();
                                        debug!(target: "lilith", "Whitelist reached max size. Removed host {}", entry.0);
                                    }
                                    // Append to the whitelist.
                                    debug!(target: "lilith", "Adding peer {} to whitelist", host);
                                    whitelist.push((host.clone(), last_seen));

                                    // Sort whitelist by last_seen.
                                    whitelist.sort_unstable_by_key(|entry| entry.1);
                                }
                                Err(e) => {
                                    debug!(target: "lilith", "Handshake failure! {}", e);
                                }
                            }
                        }

                        Err(e) => {
                            debug!(target: "lilith", "Failed to connect to {}, ({})", host, e);
                        }
                    }

                });
            }

            join_all(tasks).await;
        }
    }

    ///// Internal task to run a periodic purge of unreachable hosts
    ///// for a specific P2P network.
    //async fn periodic_purge(name: String, p2p: P2pPtr, ex: Arc<Executor<'_>>) -> Result<()> {
    //    info!(target: "lilith", "Starting periodic host purge task for \"{}\"", name);

    //    // Initialize a growable ring buffer(VecDeque) to store known hosts
    //    let ring_buffer = Arc::new(RwLock::new(VecDeque::<Url>::new()));
    //    loop {
    //        // Wait for next purge period
    //        sleep(PURGE_PERIOD).await;
    //        debug!(target: "lilith", "[{}] The Purge has started...", name);

    //        // Check if new hosts exist and add them to the end of the ring buffer
    //        let mut lock = ring_buffer.write().await;
    //        let hosts = p2p.clone().hosts().whitelist_fetch_all().await;
    //        if hosts.len() != lock.len() {
    //            // Since hosts are stored in a HashSet we have to check all of them
    //            for host in hosts {
    //                if !lock.contains(&host) {
    //                    lock.push_back(host);
    //                }
    //            }
    //        }

    //        // Pick first up to PROBE_HOSTS_N hosts from the ring buffer
    //        let mut purgers = vec![];
    //        let mut index = 0;
    //        while index <= PROBE_HOSTS_N {
    //            match lock.pop_front() {
    //                Some(host) => purgers.push(host),
    //                None => break,
    //            };
    //            index += 1;
    //        }

    //        // Try to connect to them. If we can't reach them, remove them from our set.
    //        let purgers_str: Vec<&str> = purgers.iter().map(|x| x.as_str()).collect();
    //        debug!(target: "lilith", "[{}] Got: {:?}", name, purgers_str);

    //        let mut tasks = vec![];

    //        for host in &purgers {
    //            let p2p_ = p2p.clone();
    //            let ex_ = ex.clone();
    //            let ring_buffer_ = ring_buffer.clone();
    //            tasks.push(async move {
    //                let session_out = p2p_.session_outbound();
    //                let session_weak = Arc::downgrade(&session_out);

    //                let connector = Connector::new(p2p_.settings(), session_weak);
    //                debug!(target: "lilith", "Connecting to {}", host);
    //                match connector.connect(host).await {
    //                    Ok((_url, channel)) => {
    //                        debug!(target: "lilith", "Connected successfully!");
    //                        let proto_ver = ProtocolVersion::new(
    //                            channel.clone(),
    //                            p2p_.settings().clone(),
    //                            //p2p_.hosts().clone(),
    //                        )
    //                        .await;

    //                        let handshake_task = session_out.perform_handshake_protocols(
    //                            proto_ver,
    //                            channel.clone(),
    //                            ex_.clone(),
    //                        );

    //                        channel.clone().start(ex_.clone());

    //                        match handshake_task.await {
    //                            Ok(()) => {
    //                                debug!(target: "lilith", "Handshake success! Stopping channel.");
    //                                channel.stop().await;
    //                                // Push host back to the ring buffer
    //                                ring_buffer_.write().await.push_back(host.clone());
    //                            }
    //                            Err(e) => {
    //                                debug!(target: "lilith", "Handshake failure! {}", e);
    //                                p2p_.hosts().remove(host).await;
    //                            }
    //                        }
    //                    }

    //                    Err(e) => {
    //                        debug!(target: "lilith", "Failed to connect to {}, removing from set ({})", host, e);
    //                        // Remove from hosts set
    //                        p2p_.hosts().remove(host).await;
    //                    }
    //                }
    //            });
    //        }

    //        join_all(tasks).await;
    //    }
    //}

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
impl RequestHandler for Lilith {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        match req.method.as_str() {
            "ping" => return self.pong(req.id, req.params).await,
            "spawns" => return self.spawns(req.id, req.params).await,
            _ => return JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'_, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

///// Attempt to read existing hosts tsv
//fn load_hosts(path: &Path, networks: &[&str]) -> HashMap<String, HashSet<Url>> {
//    let mut saved_hosts = HashMap::new();
//
//    let contents = load_file(path);
//    if let Err(e) = contents {
//        warn!(target: "lilith", "Failed retrieving saved hosts: {}", e);
//        return saved_hosts
//    }
//
//    for line in contents.unwrap().lines() {
//        let data: Vec<&str> = line.split('\t').collect();
//        if networks.contains(&data[0]) {
//            let mut hosts = match saved_hosts.get(data[0]) {
//                Some(hosts) => hosts.clone(),
//                None => HashSet::new(),
//            };
//
//            let url = match Url::parse(data[1]) {
//                Ok(u) => u,
//                Err(e) => {
//                    warn!(target: "lilith", "Skipping malformed url: {} ({})", data[1], e);
//                    continue
//                }
//            };
//
//            hosts.insert(url);
//            saved_hosts.insert(data[0].to_string(), hosts);
//        }
//    }
//
//    saved_hosts
//}

fn load_hosts(path: &Path, networks: &[&str]) -> HashMap<String, Vec<(Url, u64)>> {
    let mut saved_hosts = HashMap::new();

    let contents = load_file(path);
    if let Err(e) = contents {
        warn!(target: "lilith", "Failed retrieving saved hosts: {}", e);
        return saved_hosts
    }

    for line in contents.unwrap().lines() {
        let data: Vec<&str> = line.split('\t').collect();
        debug!(target: "lilith", "::load_hosts()::data\"{:?}\"", data);
        if networks.contains(&data[0]) {
            let mut hosts = match saved_hosts.get(data[0]) {
                Some(hosts) => hosts.clone(),
                None => Vec::new(),
            };

            let url = match Url::parse(data[1]) {
                Ok(u) => u,
                Err(e) => {
                    warn!(target: "lilith", "Skipping malformed url: {} ({})", data[1], e);
                    continue
                }
            };

            let last_seen = match data[2].parse::<u64>() {
                Ok(u) => u,
                Err(e) => {
                    warn!(target: "lilith", "Skipping malformed timestamp: {} ({})", data[2], e);
                    continue
                }
            };
            hosts.push((url, last_seen));
            saved_hosts.insert(data[0].to_string(), hosts);
        }
    }

    saved_hosts
}

//async fn save_hosts(path: &Path, networks: &[Spawn]) {
//    let mut tsv = String::new();
//
//    for spawn in networks {
//        for host in spawn.p2p.hosts().fetch_all().await {
//            tsv.push_str(&format!("{}\t{}\n", spawn.name, host.as_str()));
//        }
//    }
//
//    if !tsv.eq("") {
//        info!(target: "lilith", "Saving current hosts of spawned networks to: {:?}", path);
//        if let Err(e) = save_file(path, &tsv) {
//            error!(target: "lilith", "Failed saving hosts: {}", e);
//        }
//    }
//}

async fn save_hosts(path: &Path, networks: &[Spawn]) {
    let mut tsv = String::new();

    for spawn in networks {
        for (host, last_seen) in spawn.p2p.hosts().whitelist_fetch_all().await {
            tsv.push_str(&format!("{}\t{}\t{}\n", spawn.name, host.as_str(), last_seen));
        }
    }

    if !tsv.eq("") {
        info!(target: "lilith", "Saving current hosts of spawned networks to: {:?}", path);
        if let Err(e) = save_file(path, &tsv) {
            error!(target: "lilith", "Failed saving hosts: {}", e);
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
                info!(target: "lilith", "Found configuration for network: {}", net.0);
                let table = net.1.as_table().unwrap();
                if !table.contains_key("accept_addrs") {
                    warn!(target: "lilith", "Network accept addrs are mandatory, skipping network.");
                    continue
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

                let net_info = NetInfo { accept_addrs, seeds, peers, version, localnet };
                ret.insert(name, net_info);
            }
        }
    }

    Ok(ret)
}

//async fn spawn_net(
//    name: String,
//    info: &NetInfo,
//    saved_hosts: Vec<(Url, u64)>,
//    ex: Arc<Executor<'static>>,
//) -> Result<Spawn> {
//    let mut listen_urls = vec![];
//
//    // Configure listen addrs for this network
//    for url in &info.accept_addrs {
//        listen_urls.push(url.clone());
//    }
//
//    // P2P network settings
//    let settings = net::Settings {
//        inbound_addrs: listen_urls.clone(),
//        seeds: info.seeds.clone(),
//        peers: info.peers.clone(),
//        outbound_connections: 0,
//        outbound_connect_timeout: 30,
//        inbound_connections: 512,
//        app_version: info.version.clone(),
//        localnet: info.localnet,
//        allowed_transports: vec![
//            "tcp".to_string(),
//            "tcp+tls".to_string(),
//            "tor".to_string(),
//            "tor+tls".to_string(),
//            "nym".to_string(),
//            "nym+tls".to_string(),
//        ],
//        ..Default::default()
//    };
//
//    // Create P2P instance
//    let p2p = P2p::new(settings, ex.clone()).await;
//
//    // Fill db with cached hosts
//    let hosts: Vec<(Url, u64)> = saved_hosts.iter().cloned().collect();
//    p2p.hosts().greylist_store(&hosts).await;
//
//    let addrs_str: Vec<&str> = listen_urls.iter().map(|x| x.as_str()).collect();
//    info!(target: "lilith", "Starting seed network node for \"{}\" on {:?}", name, addrs_str);
//    p2p.clone().start().await?;
//
//    let spawn = Spawn { name, p2p };
//    Ok(spawn)
//}
//
async fn spawn_net(
    name: String,
    info: &NetInfo,
    saved_hosts: &Vec<(Url, u64)>,
    ex: Arc<Executor<'static>>,
) -> Result<Spawn> {
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
    let p2p = P2p::new(settings, ex.clone()).await;

    // Fill db with cached hosts
    let hosts: Vec<(Url, u64)> = saved_hosts.iter().cloned().collect();
    p2p.hosts().greylist_store(&hosts).await;

    let addrs_str: Vec<&str> = listen_urls.iter().map(|x| x.as_str()).collect();
    info!(target: "lilith", "Starting seed network node for \"{}\" on {:?}", name, addrs_str);
    p2p.clone().start().await?;

    let spawn = Spawn { name, p2p };
    Ok(spawn)
}

//async_daemonize!(realmain);
//async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
//    // Pick up network settings from the TOML config
//    let cfg_path = get_config_path(args.config, CONFIG_FILE)?;
//    let toml_contents = std::fs::read_to_string(cfg_path)?;
//    let configured_nets = parse_configured_networks(&toml_contents)?;
//
//    if configured_nets.is_empty() {
//        error!(target: "lilith", "No networks are enabled in config");
//        exit(1);
//    }
//
//    // Retrieve any saved hosts for configured networks
//    let net_names: Vec<&str> = configured_nets.keys().map(|x| x.as_str()).collect();
//    let saved_hosts = load_hosts(&expand_path(&args.hosts_file)?, &net_names);
//
//    // Spawn configured networks
//    let mut networks = vec![];
//    for (name, info) in &configured_nets {
//        // TODO: Here we could actually differentiate between network versions
//        // e.g. p2p_v3, p2p_v4, etc. Therefore we can spawn multiple networks
//        // and they would all be version-checked, so we avoid mismatches when
//        // seeding peers.
//        match spawn_net(
//            name.to_string(),
//            info,
//            saved_hosts.get(name).unwrap_or(&HashSet::new()),
//            ex.clone(),
//        )
//        .await
//        {
//            Ok(spawn) => networks.push(spawn),
//            Err(e) => {
//                error!(target: "lilith", "Failed to start P2P network seed for \"{}\": {}", name, e);
//                exit(1);
//            }
//        }
//    }
//
//    // Set up main daemon and background tasks
//    let lilith = Arc::new(Lilith { networks, rpc_connections: Mutex::new(HashSet::new()) });
//    let mut periodic_tasks = HashMap::new();
//    for network in &lilith.networks {
//        let name = network.name.clone();
//        let task = StoppableTask::new();
//        task.clone().start(
//            Lilith::periodic_purge(name.clone(), network.p2p.clone(), ex.clone()),
//            |res| async move {
//                match res {
//                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
//                    Err(e) => error!(target: "lilith", "Failed starting periodic task for \"{}\": {}", name, e),
//                }
//            },
//            Error::DetachedTaskStopped,
//            ex.clone(),
//        );
//        periodic_tasks.insert(network.name.clone(), task);
//    }
//
//    // JSON-RPC server
//    info!(target: "lilith", "Starting JSON-RPC server on {}", args.rpc_listen);
//    let lilith_ = lilith.clone();
//    let rpc_task = StoppableTask::new();
//    rpc_task.clone().start(
//        listen_and_serve(args.rpc_listen, lilith.clone(), None, ex.clone()),
//        |res| async move {
//            match res {
//                Ok(()) | Err(Error::RpcServerStopped) => lilith_.stop_connections().await,
//                Err(e) => error!(target: "lilith", "Failed starting JSON-RPC server: {}", e),
//            }
//        },
//        Error::RpcServerStopped,
//        ex.clone(),
//    );
//
//    // Signal handling for graceful termination.
//    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
//    signals_handler.wait_termination(signals_task).await?;
//    info!(target: "lilith", "Caught termination signal, cleaning up and exiting...");
//
//    // Save in-memory hosts to tsv file
//    save_hosts(&expand_path(&args.hosts_file)?, &lilith.networks).await;
//
//    info!(target: "lilith", "Stopping JSON-RPC server...");
//    rpc_task.stop().await;
//
//    // Cleanly stop p2p networks
//    for spawn in &lilith.networks {
//        info!(target: "lilith", "Stopping \"{}\" periodic task", spawn.name);
//        periodic_tasks.get(&spawn.name).unwrap().stop().await;
//        info!(target: "lilith", "Stopping \"{}\" P2P", spawn.name);
//        spawn.p2p.stop().await;
//    }
//
//    info!(target: "lilith", "Bye!");
//    Ok(())
//}

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
            saved_hosts.get(name).unwrap_or(&Vec::new()),
            ex.clone(),
        )
        .await
        {
            Ok(spawn) => networks.push(spawn),
            Err(e) => {
                error!(target: "lilith", "Failed to start P2P network seed for \"{}\": {}", name, e);
                exit(1);
            }
        }
    }

    // Set up main daemon and background tasks
    let lilith = Arc::new(Lilith { networks, rpc_connections: Mutex::new(HashSet::new()) });
    let mut periodic_tasks = HashMap::new();
    for network in &lilith.networks {
        let name = network.name.clone();
        let task = StoppableTask::new();
        task.clone().start(
            Lilith::refresh_whitelist(name.clone(), network.p2p.clone(), ex.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "lilith", "Failed starting periodic task for \"{}\": {}", name, e),
                }
            },
            Error::DetachedTaskStopped,
            ex.clone(),
        );
        periodic_tasks.insert(network.name.clone(), task);
    }

    // JSON-RPC server
    info!(target: "lilith", "Starting JSON-RPC server on {}", args.rpc_listen);
    let lilith_ = lilith.clone();
    let rpc_task = StoppableTask::new();
    rpc_task.clone().start(
        listen_and_serve(args.rpc_listen, lilith.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => lilith_.stop_connections().await,
                Err(e) => error!(target: "lilith", "Failed starting JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "lilith", "Caught termination signal, cleaning up and exiting...");

    // Save in-memory hosts to tsv file
    save_hosts(&expand_path(&args.hosts_file)?, &lilith.networks).await;

    info!(target: "lilith", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    // Cleanly stop p2p networks
    for spawn in &lilith.networks {
        info!(target: "lilith", "Stopping \"{}\" periodic task", spawn.name);
        periodic_tasks.get(&spawn.name).unwrap().stop().await;
        info!(target: "lilith", "Stopping \"{}\" P2P", spawn.name);
        spawn.p2p.stop().await;
    }

    info!(target: "lilith", "Bye!");
    Ok(())
}
