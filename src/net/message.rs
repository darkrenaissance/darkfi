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

use darkfi_serial::{
    async_trait, AsyncDecodable, AsyncEncodable, Decodable, Encodable, SerialDecodable,
    SerialEncodable, VarInt,
};
use log::trace;
use smol::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use url::Url;

use crate::{Error, Result};

const MAGIC_BYTES: [u8; 4] = [0xd9, 0xef, 0xb6, 0x7d];

/// Generic message template.
pub trait Message: 'static + Send + Sync + Encodable + Decodable {
    const NAME: &'static str;
}

#[macro_export]
macro_rules! impl_p2p_message {
    ($st:ty, $nm:expr) => {
        impl Message for $st {
            const NAME: &'static str = $nm;
        }
    };
}

/// Outbound keepalive message.
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct PingMessage {
    pub nonce: u16,
}
impl_p2p_message!(PingMessage, "ping");

/// Inbound keepalive message.
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct PongMessage {
    pub nonce: u16,
}
impl_p2p_message!(PongMessage, "pong");

/// Requests address of outbound connecction.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct GetAddrsMessage {
    /// Maximum number of addresses with preferred
    /// transports to receive. Response vector will
    /// also containg addresses without the preferred
    /// transports, so its size will be 2 * max.
    pub max: u32,
    /// Preferred addresses transports
    pub transports: Vec<String>,
}
impl_p2p_message!(GetAddrsMessage, "getaddr");

/// Sends address information to inbound connection.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AddrsMessage {
    pub addrs: Vec<(Url, u64)>,
}

impl_p2p_message!(AddrsMessage, "addr");

/// Requests version information of outbound connection.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VersionMessage {
    /// Only used for debugging. Compromises privacy when set.
    pub node_id: String,
}
impl_p2p_message!(VersionMessage, "version");

/// Sends version information to inbound connection.
/// Response to `VersionMessage`.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerackMessage {
    /// App version
    pub app_version: semver::Version,
}
impl_p2p_message!(VerackMessage, "verack");

/// Packets are the base type read from the network.
/// Converted to messages and passed to event loop.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct Packet {
    pub command: String,
    pub payload: Vec<u8>,
}

/// Reads and decodes an inbound payload from the given async stream.
/// Returns decoded [`Packet`].
pub async fn read_packet<R: AsyncRead + Unpin + Send + Sized>(stream: &mut R) -> Result<Packet> {
    // Packets should have a 4 byte header of magic digits.
    // This is used for network debugging.
    let mut magic = [0u8; 4];
    trace!(target: "net::message", "Reading magic...");
    stream.read_exact(&mut magic).await?;

    trace!(target: "net::message", "Read magic {:?}", magic);
    if magic != MAGIC_BYTES {
        trace!(target: "net::message", "Error: Magic bytes mismatch");
        return Err(Error::MalformedPacket)
    }

    // The type of the message.
    let command_len = VarInt::decode_async(stream).await?.0 as usize;
    let mut cmd = vec![0u8; command_len];
    stream.read_exact(&mut cmd).await?;
    let command = String::from_utf8(cmd)?;
    trace!(target: "net::message", "Read command: {}", command);

    // The message-dependent data (see message types)
    let payload_len = VarInt::decode_async(stream).await?.0 as usize;
    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload).await?;
    trace!(target: "net::message", "Read payload {} bytes", payload_len);

    Ok(Packet { command, payload })
}

/// Sends an outbound packet by writing data to the given async stream.
/// Returns the total written bytes.
pub async fn send_packet<W: AsyncWrite + Unpin + Send + Sized>(
    stream: &mut W,
    packet: Packet,
) -> Result<usize> {
    assert!(!packet.command.is_empty());
    //assert!(!packet.payload.is_empty());
    assert!(std::mem::size_of::<usize>() <= std::mem::size_of::<u64>());

    let mut written: usize = 0;

    trace!(target: "net::message", "Sending magic...");
    stream.write_all(&MAGIC_BYTES).await?;
    written += MAGIC_BYTES.len();
    trace!(target: "net::message", "Sent magic");

    trace!(target: "net::message", "Sending command...");
    written += VarInt(packet.command.len() as u64).encode_async(stream).await?;
    let cmd_ref = packet.command.as_bytes();
    stream.write_all(cmd_ref).await?;
    written += cmd_ref.len();
    trace!(target: "net::message", "Sent command: {}", packet.command);

    trace!(target: "net::message", "Sending payload...");
    written += VarInt(packet.payload.len() as u64).encode_async(stream).await?;
    stream.write_all(&packet.payload).await?;
    written += packet.payload.len();
    trace!(target: "net::message", "Sent payload {} bytes", packet.payload.len() as u64);

    stream.flush().await?;

    Ok(written)
}
