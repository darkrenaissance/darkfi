use std::sync::Arc;

use serde::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

/// Atomic pointer to network settings.
pub type SettingsPtr = Arc<Settings>;

/// Defines the network settings.
#[derive(Clone, Debug)]
pub struct Settings {
    pub inbound: Option<Url>,
    pub outbound_connections: u32,
    pub manual_attempt_limit: u32,
    pub seed_query_timeout_seconds: u32,
    pub connect_timeout_seconds: u32,
    pub channel_handshake_seconds: u32,
    pub channel_heartbeat_seconds: u32,
    pub external_addr: Option<Url>,
    pub peers: Vec<Url>,
    pub seeds: Vec<Url>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            inbound: None,
            outbound_connections: 0,
            manual_attempt_limit: 0,
            seed_query_timeout_seconds: 8,
            connect_timeout_seconds: 10,
            channel_handshake_seconds: 4,
            channel_heartbeat_seconds: 10,
            external_addr: None,
            peers: Vec::new(),
            seeds: Vec::new(),
        }
    }
}

/// Defines the network settings.
#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[structopt()]
pub struct SettingsOpt {
    /// P2P accept address
    #[structopt(long = "accept")]
    pub inbound: Option<Url>,

    /// Connection slots
    #[structopt(long = "slots")]
    pub outbound_connections: Option<u32>,

    /// P2P external address
    #[structopt(long)]
    pub external_addr: Option<Url>,

    /// Peer nodes to connect to
    #[serde(default)]
    #[structopt(long)]
    pub peers: Vec<Url>,

    /// Seed nodes to connect to
    #[serde(default)]
    #[structopt(long)]
    pub seeds: Vec<Url>,

    #[structopt(skip)]
    pub manual_attempt_limit: Option<u32>,
    #[structopt(skip)]
    pub seed_query_timeout_seconds: Option<u32>,
    #[structopt(skip)]
    pub connect_timeout_seconds: Option<u32>,
    #[structopt(skip)]
    pub channel_handshake_seconds: Option<u32>,
    #[structopt(skip)]
    pub channel_heartbeat_seconds: Option<u32>,
}

impl Into<Settings> for SettingsOpt {
    fn into(self) -> Settings {
        Settings {
            inbound: self.inbound,
            outbound_connections: self.outbound_connections.unwrap_or(0),
            manual_attempt_limit: self.manual_attempt_limit.unwrap_or(0),
            seed_query_timeout_seconds: self.seed_query_timeout_seconds.unwrap_or(8),
            connect_timeout_seconds: self.connect_timeout_seconds.unwrap_or(10),
            channel_handshake_seconds: self.channel_handshake_seconds.unwrap_or(4),
            channel_heartbeat_seconds: self.channel_heartbeat_seconds.unwrap_or(10),
            external_addr: self.external_addr,
            peers: self.peers,
            seeds: self.seeds,
        }
    }
}
