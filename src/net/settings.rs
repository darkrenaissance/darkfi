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

use std::sync::Arc;

use serde::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

use crate::net::transport::TransportName;

/// Atomic pointer to network settings.
pub type SettingsPtr = Arc<Settings>;

/// Default settings for the network. Can be manually configured.
#[derive(Clone, Debug)]
pub struct Settings {
    /// P2P accept addresses node listens to for inbound connections
    pub inbound: Vec<Url>,
    /// Outbound connection slots number
    pub outbound_connections: u32,
    /// Manual connections retry limit, 0 for forever looping
    pub manual_attempt_limit: u32,
    /// Seed connection establishment timeout
    pub seed_query_timeout_seconds: u32,
    /// Connection establishment timeout
    pub connect_timeout_seconds: u32,
    /// Exchange versions (handshake) timeout
    pub channel_handshake_seconds: u32,
    /// Ping-pong exhange execution interval
    pub channel_heartbeat_seconds: u32,
    /// Try to fill an outbound slot interval
    pub outbound_retry_seconds: u64,
    /// P2P external addresses node advertises so other peers can reach us
    /// and connect to us, as long us inbound addresses are also configured
    pub external_addr: Vec<Url>,
    /// Peer nodes to manually connect to
    pub peers: Vec<Url>,
    /// Seed nodes to connect to for peers retrieval and/or advertising our own
    /// external address
    pub seeds: Vec<Url>,
    /// Only used for debugging. Compromises privacy when set.
    pub node_id: String,
    /// Application version, used for verification between peers
    pub app_version: Option<String>,
    /// Prefered transports for outbound connections
    pub outbound_transports: Vec<TransportName>,
    /// Allow localnet hosts
    pub localnet: bool,
    /// Enable peer discovery
    pub peer_discovery: bool,
    /// Enable channel logging
    pub channel_log: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            inbound: Vec::new(),
            outbound_connections: 0,
            manual_attempt_limit: 0,
            seed_query_timeout_seconds: 8,
            connect_timeout_seconds: 10,
            channel_handshake_seconds: 4,
            channel_heartbeat_seconds: 10,
            outbound_retry_seconds: 20,
            external_addr: Vec::new(),
            peers: Vec::new(),
            seeds: Vec::new(),
            node_id: String::new(),
            app_version: Some(option_env!("CARGO_PKG_VERSION").unwrap_or("").to_string()),
            outbound_transports: get_outbound_transports(vec![]),
            localnet: false,
            peer_discovery: true,
            channel_log: false,
        }
    }
}

/// Defines the network settings.
#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[structopt()]
pub struct SettingsOpt {
    /// P2P accept addresses node listens to for inbound connections
    #[serde(default)]
    #[structopt(long = "accept")]
    pub inbound: Vec<Url>,

    /// Outbound connection slots number
    #[structopt(long = "slots")]
    pub outbound_connections: Option<u32>,

    /// P2P external addresses node advertises so other peers can reach us
    /// and connect to us, as long us inbound addresses are also configured
    #[serde(default)]
    #[structopt(long)]
    pub external_addr: Vec<Url>,

    /// Peer nodes to manually connect to
    #[serde(default)]
    #[structopt(long)]
    pub peers: Vec<Url>,

    /// Seed nodes to connect to for peers retrieval and/or advertising our own
    /// external address
    #[serde(default)]
    #[structopt(long)]
    pub seeds: Vec<Url>,

    /// Manual connections retry limit
    #[structopt(skip)]
    pub manual_attempt_limit: Option<u32>,

    /// Seed connection establishment timeout
    #[structopt(skip)]
    pub seed_query_timeout_seconds: Option<u32>,

    /// Connection establishment timeout
    #[structopt(skip)]
    pub connect_timeout_seconds: Option<u32>,

    /// Exchange versions (handshake) timeout
    #[structopt(skip)]
    pub channel_handshake_seconds: Option<u32>,

    /// Ping-pong exhange execution interval
    #[structopt(skip)]
    pub channel_heartbeat_seconds: Option<u32>,

    /// Try to fill an outbound slot interval
    #[structopt(skip)]
    pub outbound_retry_seconds: Option<u64>,

    /// Only used for debugging. Compromises privacy when set.
    #[serde(default)]
    #[structopt(skip)]
    pub node_id: String,

    /// Application version, used for verification between peers
    #[serde(default)]
    #[structopt(skip)]
    pub app_version: Option<String>,

    /// Prefered transports for outbound connections
    #[serde(default)]
    #[structopt(long = "transports")]
    pub outbound_transports: Vec<String>,

    /// Allow localnet hosts
    #[serde(default)]
    #[structopt(long)]
    pub localnet: bool,

    /// Enable peer discovery
    #[serde(default = "default_as_true")]
    #[structopt(long)]
    pub peer_discovery: bool,

    /// Enable channel logging
    #[serde(default)]
    #[structopt(long)]
    pub channel_log: bool,
}

impl From<SettingsOpt> for Settings {
    fn from(settings_opt: SettingsOpt) -> Self {
        Self {
            inbound: settings_opt.inbound,
            outbound_connections: settings_opt.outbound_connections.unwrap_or(0),
            manual_attempt_limit: settings_opt.manual_attempt_limit.unwrap_or(0),
            seed_query_timeout_seconds: settings_opt.seed_query_timeout_seconds.unwrap_or(8),
            connect_timeout_seconds: settings_opt.connect_timeout_seconds.unwrap_or(10),
            channel_handshake_seconds: settings_opt.channel_handshake_seconds.unwrap_or(4),
            channel_heartbeat_seconds: settings_opt.channel_heartbeat_seconds.unwrap_or(10),
            outbound_retry_seconds: settings_opt.outbound_retry_seconds.unwrap_or(1200),
            external_addr: settings_opt.external_addr,
            peers: settings_opt.peers,
            seeds: settings_opt.seeds,
            node_id: settings_opt.node_id,
            app_version: settings_opt.app_version,
            outbound_transports: get_outbound_transports(settings_opt.outbound_transports),
            localnet: settings_opt.localnet,
            peer_discovery: settings_opt.peer_discovery,
            channel_log: settings_opt.channel_log,
        }
    }
}

/// Auxiliary function to convert outbound transport Vec<String>
/// to Vec<TransportName>, using defaults if empty.
pub fn get_outbound_transports(opt_outbound_transports: Vec<String>) -> Vec<TransportName> {
    let mut outbound_transports = vec![];
    for transport in opt_outbound_transports {
        let transport_name = TransportName::try_from(transport.as_str()).unwrap();
        outbound_transports.push(transport_name);
    }

    if outbound_transports.is_empty() {
        let tls = TransportName::Tcp(Some("tls".into()));
        outbound_transports.push(tls);
        let tcp = TransportName::Tcp(None);
        outbound_transports.push(tcp);
    }

    outbound_transports
}

/// Auxiliary function to set serde bool value to true.
fn default_as_true() -> bool {
    true
}
