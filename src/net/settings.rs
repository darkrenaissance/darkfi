use std::sync::Arc;

use serde::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

use crate::net::TransportName;

/// Atomic pointer to network settings.
pub type SettingsPtr = Arc<Settings>;

/// Default settings for the network. Can be manually configured.
#[derive(Clone, Debug)]
pub struct Settings {
    pub inbound: Vec<Url>,
    pub outbound_connections: u32,
    pub manual_attempt_limit: u32,
    pub seed_query_timeout_seconds: u32,
    pub connect_timeout_seconds: u32,
    pub channel_handshake_seconds: u32,
    pub channel_heartbeat_seconds: u32,
    pub outbound_retry_seconds: u64,
    pub external_addr: Vec<Url>,
    pub peers: Vec<Url>,
    pub seeds: Vec<Url>,
    pub node_id: String,
    pub app_version: Option<String>,
    pub outbound_transports: Vec<TransportName>,
    pub localnet: bool,
    pub peer_discovery: bool,
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
        }
    }
}

/// Defines the network settings.
#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[structopt()]
pub struct SettingsOpt {
    /// P2P accept addresses
    #[serde(default)]
    #[structopt(long = "accept")]
    pub inbound: Vec<Url>,

    /// Connection slots
    #[structopt(long = "slots")]
    pub outbound_connections: Option<u32>,

    /// P2P external addresses
    #[serde(default)]
    #[structopt(long)]
    pub external_addr: Vec<Url>,

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

    #[serde(default)]
    #[structopt(skip)]
    pub app_version: Option<String>,

    /// Prefered transports for outbound connections
    #[serde(default)]
    #[structopt(long = "transports")]
    pub outbound_transports: Vec<String>,

    /// Enable localnet hosts
    #[serde(default)]
    #[structopt(long)]
    pub localnet: bool,

    /// Enable peer discovery
    #[serde(default = "default_as_true")]
    #[structopt(long)]
    pub peer_discovery: bool,
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
