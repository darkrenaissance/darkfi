use fxhash::FxHashMap;
use log::{info, warn};
use serde_derive::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use toml::Value;
use url::Url;

use darkfi::{cli_desc, Result};

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "lilith", about = cli_desc!())]
pub struct Args {
    #[structopt(long, default_value = "tcp://127.0.0.1:18927")]
    /// JSON-RPC listen URL
    pub rpc_listen: Url,

    #[structopt(short, long)]
    /// Configuration file to use
    pub config: Option<String>,

    #[structopt(long)]
    /// Daemon published urls, common for all enabled networks (repeatable flag)
    pub urls: Vec<Url>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    pub verbose: u8,
}

/// Defines the network specific settings
#[derive(Clone)]
pub struct NetInfo {
    /// Specific port the network will use
    pub port: u16,
    /// Connect to seeds (repeatable flag)
    pub seeds: Vec<Url>,
    /// Connect to peers (repeatable flag)
    pub peers: Vec<Url>,
}

/// Parse a TOML string for any configured network and return
/// a map containing said configurations.
///
/// ```toml
/// [network."darkfid_sync"]
/// port = 33032
/// seeds = []
/// peers = []
/// ```
pub fn parse_configured_networks(data: &str) -> Result<FxHashMap<String, NetInfo>> {
    let mut ret = FxHashMap::default();

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

                let net_info = NetInfo { port, seeds, peers };
                ret.insert(name, net_info);
            }
        }
    };

    Ok(ret)
}
