use std::sync::Arc;

use serde::Deserialize;
use structopt::StructOpt;
use url::Url;

/// Atomic pointer to network settings.
pub type SettingsPtr = Arc<Settings>;

/// Defines the network settings.
#[derive(Clone, Debug, Deserialize, StructOpt)]
#[structopt()]
pub struct Settings {
    #[structopt(short, long)]
    pub inbound: Option<Url>,
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
    pub external_addr: Option<Url>,
    #[structopt(short, long)]
    pub peers: Vec<Url>,
    #[structopt(short, long)]
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
