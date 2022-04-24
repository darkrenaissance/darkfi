use std::{net::SocketAddr, sync::Arc};

use serde::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;

/// Atomic pointer to network settings.
pub type SettingsPtr = Arc<Settings>;

/// Defines the network settings.
#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[structopt()]
pub struct Settings {
    #[structopt(short, long)]
    pub inbound: Option<SocketAddr>,
    #[structopt(long, default_value = "0")]
    pub outbound_connections: u32,
    #[structopt(long, default_value = "0")]
    pub manual_attempt_limit: u32,
    #[structopt(long, default_value = "8")]
    pub seed_query_timeout_seconds: u32,
    #[structopt(long, default_value = "10")]
    pub connect_timeout_seconds: u32,
    #[structopt(long, default_value = "4")]
    pub channel_handshake_seconds: u32,
    #[structopt(long, default_value = "10")]
    pub channel_heartbeat_seconds: u32,
    #[structopt(short, long)]
    pub external_addr: Option<SocketAddr>,
    #[structopt(short, long)]
    pub peers: Vec<SocketAddr>,
    #[structopt(short, long)]
    pub seeds: Vec<SocketAddr>,
}
