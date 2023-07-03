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

use async_std::sync::Arc;
use structopt::StructOpt;
use url::Url;

/// Atomic pointer to network settings
pub type SettingsPtr = Arc<Settings>;

/// P2P network settings. The scope of this is a P2P network instance
/// configured by the library user.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Only used for debugging, compromises privacy when set
    pub node_id: String,
    /// P2P accept addresses the instance listens on for inbound connections
    pub inbound_addrs: Vec<Url>,
    /// P2P external addresses the instance advertises so other peers can
    /// reach us and connect to us, as long as inbound addrs are configured
    pub external_addrs: Vec<Url>,
    /// Peer nodes to manually connect to
    pub peers: Vec<Url>,
    /// Seed nodes to connect to for peer discovery and/or adversising our
    /// own external addresses
    pub seeds: Vec<Url>,
    /// Application version, used for convenient protocol matching
    pub app_version: semver::Version,
    /// Whitelisted network transports for outbound connections
    pub allowed_transports: Vec<String>,
    /// Allow transport mixing (e.g. Tor would be allowed to connect to `tcp://`)
    pub transport_mixing: bool,
    /// Outbound connection slots number, this many connections will be
    /// attempted. (This does not include manual connections)
    pub outbound_connections: usize,
    /// Manual connections retry limit, 0 for forever looping
    pub manual_attempt_limit: usize,
    /// Seed connection establishment timeout (in seconds)
    pub seed_query_timeout: u64,
    /// Outbound connection establishment timeout (in seconds)
    pub outbound_connect_timeout: u64,
    /// Exchange versions (handshake) timeout (in seconds)
    pub channel_handshake_timeout: u64,
    /// Ping-pong exchange execution interval (in seconds)
    pub channel_heartbeat_interval: u64,
    /// Allow localnet hosts
    pub localnet: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let version = option_env!("CARGO_PKG_VERSION").unwrap_or("0.0.0");
        let app_version = semver::Version::parse(version).unwrap();

        Self {
            node_id: String::new(),
            inbound_addrs: vec![],
            external_addrs: vec![],
            peers: vec![],
            seeds: vec![],
            app_version,
            allowed_transports: vec![],
            transport_mixing: true,
            outbound_connections: 0,
            manual_attempt_limit: 0,
            seed_query_timeout: 30,
            outbound_connect_timeout: 15,
            channel_handshake_timeout: 4,
            channel_heartbeat_interval: 10,
            localnet: false,
        }
    }
}

// The following is used so we can have P2P settings configurable
// from TOML files.

/// Defines the network settings.
#[derive(Clone, Debug, serde::Deserialize, structopt::StructOpt, structopt_toml::StructOptToml)]
#[structopt()]
pub struct SettingsOpt {
    /// P2P accept address node listens to for inbound connections
    #[serde(default)]
    #[structopt(long = "accept")]
    pub inbound: Vec<Url>,

    /// Outbound connection slots number
    #[structopt(long = "slots")]
    pub outbound_connections: Option<usize>,

    /// P2P external addresses node advertises so other peers can
    /// reach us and connect to us, as long as inbound addresses
    /// are also configured
    #[serde(default)]
    #[structopt(long)]
    pub external_addrs: Vec<Url>,

    /// Peer nodes to manually connect to
    #[serde(default)]
    #[structopt(long)]
    pub peers: Vec<Url>,

    /// Seed nodes to connect to for peers retrieval and/or
    /// advertising our own external addresses
    #[serde(default)]
    #[structopt(long)]
    pub seeds: Vec<Url>,

    /// Manual connections retry limit
    #[structopt(skip)]
    pub manual_attempt_limit: Option<usize>,

    /// Seed connection establishment timeout in seconds
    #[structopt(skip)]
    pub seed_query_timeout: Option<u64>,

    /// Connection establishment timeout in seconds
    #[structopt(skip)]
    pub outbound_connect_timeout: Option<u64>,

    /// Exchange versions (handshake) timeout in seconds
    #[structopt(skip)]
    pub channel_handshake_timeout: Option<u64>,

    /// Ping-pong exchange execution interval in seconds
    #[structopt(skip)]
    pub channel_heartbeat_interval: Option<u64>,

    /// Only used for debugging. Compromises privacy when set.
    #[serde(default)]
    #[structopt(skip)]
    pub node_id: String,

    /// Preferred transports for outbound connections    
    #[serde(default)]
    #[structopt(long = "transports")]
    pub allowed_transports: Vec<String>,

    #[structopt(long)]
    pub transport_mixing: Option<bool>,

    /// Allow localnet hosts
    #[serde(default)]
    #[structopt(long)]
    pub localnet: bool,
}

impl From<SettingsOpt> for Settings {
    fn from(opt: SettingsOpt) -> Self {
        let version = option_env!("CARGO_PKG_VERSION").unwrap_or("0.0.0");
        let app_version = semver::Version::parse(version).unwrap();

        Self {
            node_id: opt.node_id,
            inbound_addrs: opt.inbound,
            external_addrs: opt.external_addrs,
            peers: opt.peers,
            seeds: opt.seeds,
            app_version,
            allowed_transports: opt.allowed_transports,
            transport_mixing: opt.transport_mixing.unwrap_or(false),
            outbound_connections: opt.outbound_connections.unwrap_or(0),
            manual_attempt_limit: opt.manual_attempt_limit.unwrap_or(0),
            seed_query_timeout: opt.seed_query_timeout.unwrap_or(30),
            outbound_connect_timeout: opt.outbound_connect_timeout.unwrap_or(15),
            channel_handshake_timeout: opt.channel_handshake_timeout.unwrap_or(4),
            channel_heartbeat_interval: opt.channel_heartbeat_interval.unwrap_or(10),
            localnet: opt.localnet,
        }
    }
}
