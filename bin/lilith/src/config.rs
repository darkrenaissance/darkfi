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

use std::collections::HashMap;

use log::{info, warn};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
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

    #[structopt(long, default_value = "~/.config/darkfi/lilith_hosts.tsv")]
    /// Hosts .tsv file to use
    pub hosts_file: String,

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
    /// Enable localnet hosts
    pub localnet: bool,
    /// Enable channel log
    pub channel_log: bool,
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
pub fn parse_configured_networks(data: &str) -> Result<HashMap<String, NetInfo>> {
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

                let channel_log = if table.contains_key("channel_log") {
                    table["channel_log"].as_bool().unwrap()
                } else {
                    false
                };

                let net_info = NetInfo { port, seeds, peers, localnet, channel_log };
                ret.insert(name, net_info);
            }
        }
    };

    Ok(ret)
}
