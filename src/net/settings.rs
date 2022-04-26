use std::{net::SocketAddr, sync::Arc};

use serde::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;

/// Atomic pointer to network settings.
pub type SettingsPtr = Arc<Settings>;

/// Defines the network settings.
#[derive(Clone, Debug)]
pub struct Settings {
    pub inbound: Option<SocketAddr>,
    pub outbound_connections: u32,
    pub manual_attempt_limit: u32,
    pub seed_query_timeout_seconds: u32,
    pub connect_timeout_seconds: u32,
    pub channel_handshake_seconds: u32,
    pub channel_heartbeat_seconds: u32,
    pub external_addr: Option<SocketAddr>,
    pub peers: Vec<SocketAddr>,
    pub seeds: Vec<SocketAddr>,
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
#[serde(default)]
#[structopt()]
pub struct SettingsOpt {
    /// P2P accept address
    #[structopt(long = "accept")]
    pub inbound: Option<SocketAddr>,

    /// Connection slots
    #[structopt(long = "slots", default_value = "0")]
    pub outbound_connections: u32,

    /// P2P external address
    #[structopt(long)]
    pub external_addr: Option<SocketAddr>,

    /// Peer nodes to connect to
    #[structopt(long)]
    pub peers: Vec<SocketAddr>,

    /// Seed nodes to connect to
    #[structopt(long)]
    pub seeds: Vec<SocketAddr>,

    #[structopt(skip = 0 as u32)]
    pub manual_attempt_limit: u32,
    #[structopt(skip = 8 as u32)]
    pub seed_query_timeout_seconds: u32,
    #[structopt(skip = 10 as u32)]
    pub connect_timeout_seconds: u32,
    #[structopt(skip = 4 as u32)]
    pub channel_handshake_seconds: u32,
    #[structopt(skip = 10 as u32)]
    pub channel_heartbeat_seconds: u32,
}

impl Into<Settings> for SettingsOpt {
    fn into(self) -> Settings {
        Settings {
            inbound: self.inbound,
            outbound_connections: self.outbound_connections,
            manual_attempt_limit: self.manual_attempt_limit,
            seed_query_timeout_seconds: self.seed_query_timeout_seconds,
            connect_timeout_seconds: self.connect_timeout_seconds,
            channel_handshake_seconds: self.channel_handshake_seconds,
            channel_heartbeat_seconds: self.channel_heartbeat_seconds,
            external_addr: self.external_addr,
            peers: self.peers,
            seeds: self.seeds,
        }
    }
}
