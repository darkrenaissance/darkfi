use std::sync::Arc;

use serde::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

/// Atomic pointer to network settings.
pub type SettingsPtr = Arc<Settings>;

// TODO: better documentation
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
    pub outbound_retry_seconds: u64,
    pub external_addr: Option<Url>,
    pub peers: Vec<Url>,
    pub seeds: Vec<Url>,
    pub node_id: String,
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
            outbound_retry_seconds: 1200,
            external_addr: None,
            peers: Vec::new(),
            seeds: Vec::new(),
            node_id: String::new(),
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
    #[structopt(skip)]
    pub outbound_retry_seconds: Option<u64>,

    #[serde(default)]
    #[structopt(skip)]
    pub node_id: String,
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
        }
    }
}
