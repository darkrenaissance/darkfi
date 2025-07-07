/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::net::Ipv6Addr;

use darkfi_serial::{
    async_trait, serialize_async, AsyncDecodable, AsyncEncodable, SerialDecodable, SerialEncodable,
};
use url::{Host, Url};

use crate::{net::metering::MeteringConfiguration, util::time::NanoTimestamp};

/// Generic message template.
pub trait Message: 'static + Send + Sync + AsyncDecodable + AsyncEncodable {
    const NAME: &'static str;
    /// Message bytes vector length limit.
    /// Set to 0 for no limit.
    const MAX_BYTES: u64;
    /// Message metering score value.
    /// Set to 0 for no impact in metering.
    const METERING_SCORE: u64;
    /// Message metering configuration for rate limit.
    /// Use `MeteringConfiguration::default()` for no limit.
    const METERING_CONFIGURATION: MeteringConfiguration;
}

/// Generic serialized message template.
pub struct SerializedMessage {
    pub command: String,
    pub payload: Vec<u8>,
}

impl SerializedMessage {
    pub async fn new<M: Message>(message: &M) -> Self {
        Self { command: M::NAME.to_string(), payload: serialize_async(message).await }
    }
}

#[macro_export]
macro_rules! impl_p2p_message {
    ($st:ty, $nm:expr, $mb:expr, $ms:expr, $mc:expr) => {
        impl Message for $st {
            const NAME: &'static str = $nm;
            const MAX_BYTES: u64 = $mb;
            const METERING_SCORE: u64 = $ms;
            const METERING_CONFIGURATION: MeteringConfiguration = $mc;
        }
    };
}

/// Maximum command (message name) length in bytes.
pub const MAX_COMMAND_LENGTH: u8 = 255;

/// For each message configs a threshold was calculated by taking the
/// maximum number of messages in a 10 seconds window and multiply it
/// by 2 not to be strict.
pub const PING_PONG_METERING_CONFIGURATION: MeteringConfiguration = MeteringConfiguration {
    threshold: 4,
    sleep_step: 1000,
    expiry_time: NanoTimestamp::from_secs(10),
};

/// Ping-Pong messages fields size:
/// * nonce = 2
pub const PING_PONG_MAX_BYTES: u64 = 2;

/// Outbound keepalive message.
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct PingMessage {
    pub nonce: u16,
}
impl_p2p_message!(PingMessage, "ping", PING_PONG_MAX_BYTES, 1, PING_PONG_METERING_CONFIGURATION);

/// Inbound keepalive message.
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct PongMessage {
    pub nonce: u16,
}
impl_p2p_message!(PongMessage, "pong", PING_PONG_MAX_BYTES, 1, PING_PONG_METERING_CONFIGURATION);

/// Requests address of outbound connection.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct GetAddrsMessage {
    /// Maximum number of addresses with preferred
    /// transports to receive. Response vector will
    /// also contain addresses without the preferred
    /// transports, so its size will be 2 * max.
    pub max: u32,
    /// Preferred addresses transports.
    pub transports: Vec<String>,
}
pub const GET_ADDRS_METERING_CONFIGURATION: MeteringConfiguration = MeteringConfiguration {
    threshold: 6,
    sleep_step: 1000,
    expiry_time: NanoTimestamp::from_secs(10),
};

/// GetAddrs message fields size:
/// * max = 4
/// * transports = 1 (vec_len) + 4 + 4 + 4 + 4 + 4 + 8 + 8 + 8 + 8 = 53
///
/// Transports is list of all transports to be shared specified in protocol_address.
pub const GET_ADDRS_MAX_BYTES: u64 = 57;

impl_p2p_message!(
    GetAddrsMessage,
    "getaddr",
    GET_ADDRS_MAX_BYTES,
    1,
    GET_ADDRS_METERING_CONFIGURATION
);

/// Sends address information to inbound connection.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AddrsMessage {
    pub addrs: Vec<(Url, u64)>,
}
pub const ADDRS_METERING_CONFIGURATION: MeteringConfiguration = MeteringConfiguration {
    threshold: 6,
    sleep_step: 1000,
    expiry_time: NanoTimestamp::from_secs(10),
};

/// Addrs message fields size:
/// * addrs = 1 (vec_len) + (u8::MAX * 2) * 128
///
/// Url type is estimated to be max 128 bytes here and for other message below.
pub const ADDRS_MAX_BYTES: u64 = 65281;

impl_p2p_message!(AddrsMessage, "addr", ADDRS_MAX_BYTES, 1, ADDRS_METERING_CONFIGURATION);

/// Requests version information of outbound connection.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VersionMessage {
    /// Only used for debugging. Compromises privacy when set.
    pub node_id: String,
    /// Identifies protocol version being used by the node.
    pub version: semver::Version,
    /// UNIX timestamp of when the VersionMessage was created.
    pub timestamp: u64,
    /// Network address of the node receiving this message (before
    /// resolving).
    pub connect_recv_addr: Url,
    /// Network address of the node receiving this message (after
    /// resolving). Optional because only used by outbound connections.
    pub resolve_recv_addr: Option<Url>,
    /// External address of the sender node, if it exists (empty
    /// otherwise).
    pub ext_send_addr: Vec<Url>,
    /// List of features consisting of a tuple of (services, version)
    /// to be enabled for this connection.
    pub features: Vec<(String, u32)>,
}
pub const VERSION_METERING_CONFIGURATION: MeteringConfiguration = MeteringConfiguration {
    threshold: 4,
    sleep_step: 1000,
    expiry_time: NanoTimestamp::from_secs(10),
};

/// Version message fields size:
/// * node_id = 8  (this will be empty most of the time)
/// * version = 128 (look at VerackMessage for the reasoning)
/// * timestamp = 8
/// * connect_recv_addr = 128
/// * resolve_recv_addr = 1 (enum_len) + 128(url) = 129
/// * ext_send_addr = 1 (vec_len)  + 128 * 10 = 1281 (10 is a reasonable cap for number of external addresses)
/// * features = 1 (vec_len) + (32 (service_name) + 4 (service_version)) * 10 = 361 (10 features is an estimate)
pub const VERSION_MAX_BYTES: u64 = 2043;

impl_p2p_message!(VersionMessage, "version", VERSION_MAX_BYTES, 1, VERSION_METERING_CONFIGURATION);

impl VersionMessage {
    pub(in crate::net) fn get_ipv6_addr(&self) -> Option<Ipv6Addr> {
        let host = self.connect_recv_addr.host()?;
        // Check the reported address is Ipv6
        match host {
            Host::Ipv6(addr) => Some(addr),
            _ => None,
        }
    }
}

/// Sends version information to inbound connection.
/// Response to `VersionMessage`.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerackMessage {
    /// App version
    pub app_version: semver::Version,
}
pub const VERACK_METERING_CONFIGURATION: MeteringConfiguration = MeteringConfiguration {
    threshold: 4,
    sleep_step: 1000,
    expiry_time: NanoTimestamp::from_secs(10),
};

/// Verack message fields size:
/// * app_version = 24 (major = 8, minor = 8, patch = 8) + 52 (prerelease =  1(str_len) + 51(str)) + 52 (build = 1(str_len) + 51(str))
///
/// Prerelease and build strings are variable length but shouldn't be larger than 102 bytes.
pub const VERACK_MAX_BYTES: u64 = 128;

impl_p2p_message!(VerackMessage, "verack", VERACK_MAX_BYTES, 1, VERACK_METERING_CONFIGURATION);
