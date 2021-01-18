use std::sync::Arc;
use std::net::SocketAddr;

pub type SettingsPtr = Arc<Settings>;

#[derive(Clone)]
pub struct Settings {
    pub inbound: Option<SocketAddr>,
    pub outbound_connections: u32,

    pub connect_timeout_seconds: u32,
    pub channel_handshake_seconds: u32,
    pub channel_heartbeat_seconds: u32,

    pub external_addr: Option<SocketAddr>,
    pub peers: Vec<SocketAddr>,
    pub seeds: Vec<SocketAddr>,
}
