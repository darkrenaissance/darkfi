use std::net::SocketAddr;
use std::sync::Arc;

/// Atomic pointer to network settings.
pub type SettingsPtr = Arc<Settings>;

/// Default network configuration settings.
#[derive(Clone)]
pub struct Settings {
    pub inbound: Option<SocketAddr>,
    pub outbound_connections: u32,

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

