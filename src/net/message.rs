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

use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use log::debug;
use url::Url;

use crate::{Error, Result};

const MAGIC_BYTES: [u8; 4] = [0xd9, 0xef, 0xb6, 0x7d];

/// Generic message template.
pub trait Message: 'static + Encodable + Decodable + Send + Sync {
    fn name() -> &'static str;
}

/// Outbound keep-alive message.
#[derive(SerialEncodable, SerialDecodable)]
pub struct PingMessage {
    pub nonce: u32,
}

/// Inbound keep-alive message.
#[derive(SerialEncodable, SerialDecodable)]
pub struct PongMessage {
    pub nonce: u32,
}

/// Requests address of outbound connection.
#[derive(SerialEncodable, SerialDecodable)]
pub struct GetAddrsMessage {}

/// Sends address information to inbound connection. Response to GetAddrs
/// message.
#[derive(SerialEncodable, SerialDecodable)]
pub struct AddrsMessage {
    pub addrs: Vec<Url>,
}

/// Sends external address information to inbound connection.
#[derive(SerialEncodable, SerialDecodable)]
pub struct ExtAddrsMessage {
    pub ext_addrs: Vec<Url>,
}

/// Requests version information of outbound connection.
#[derive(SerialEncodable, SerialDecodable)]
pub struct VersionMessage {
    pub node_id: String,
}

/// Sends version information to inbound connection. Response to VersionMessage.
#[derive(SerialEncodable, SerialDecodable)]
pub struct VerackMessage {
    // app version
    pub app: String,
}

impl Message for PingMessage {
    fn name() -> &'static str {
        "ping"
    }
}

impl Message for PongMessage {
    fn name() -> &'static str {
        "pong"
    }
}

impl Message for GetAddrsMessage {
    fn name() -> &'static str {
        "getaddr"
    }
}

impl Message for AddrsMessage {
    fn name() -> &'static str {
        "addr"
    }
}

impl Message for ExtAddrsMessage {
    fn name() -> &'static str {
        "extaddr"
    }
}

impl Message for VersionMessage {
    fn name() -> &'static str {
        "version"
    }
}

impl Message for VerackMessage {
    fn name() -> &'static str {
        "verack"
    }
}

/// Packets are the base type read from the network. Converted to messages and
/// passed to event loop.
pub struct Packet {
    pub command: String,
    pub payload: Vec<u8>,
}

/// Reads and decodes an inbound payload.
pub async fn read_packet<R: AsyncRead + Unpin + Sized>(stream: &mut R) -> Result<Packet> {
    // Packets have a 4 byte header of magic digits
    // This is used for network debugging
    let mut magic = [0u8; 4];
    debug!(target: "net::message", "reading magic...");

    stream.read_exact(&mut magic).await?;

    debug!(target: "net::message", "read magic {:?}", magic);
    if magic != MAGIC_BYTES {
        return Err(Error::MalformedPacket)
    }

    // The type of the message
    let command_len = VarInt::decode_async(stream).await?.0 as usize;
    let mut cmd = vec![0u8; command_len];
    if command_len > 0 {
        stream.read_exact(&mut cmd).await?;
    }
    let cmd = String::from_utf8(cmd)?;
    debug!(target: "net::message", "read command: {}", cmd);

    let payload_len = VarInt::decode_async(stream).await?.0 as usize;

    // The message-dependent data (see message types)
    let mut payload = vec![0u8; payload_len];
    if payload_len > 0 {
        stream.read_exact(&mut payload).await?;
    }
    debug!(target: "net::message", "read payload {} bytes", payload_len);

    Ok(Packet { command: cmd, payload })
}

/// Sends an outbound packet by writing data to TCP stream.
pub async fn send_packet<W: AsyncWrite + Unpin + Sized>(
    stream: &mut W,
    packet: Packet,
) -> Result<()> {
    debug!(target: "net::message", "sending magic...");
    stream.write_all(&MAGIC_BYTES).await?;
    debug!(target: "net::message", "sent magic...");

    VarInt(packet.command.len() as u64).encode_async(stream).await?;
    assert!(!packet.command.is_empty());
    stream.write_all(packet.command.as_bytes()).await?;
    debug!(target: "net::message", "sent command: {}", packet.command);

    assert_eq!(std::mem::size_of::<usize>(), std::mem::size_of::<u64>());
    VarInt(packet.payload.len() as u64).encode_async(stream).await?;

    if !packet.payload.is_empty() {
        stream.write_all(&packet.payload).await?;
    }
    debug!(target: "net::message", "sent payload {} bytes", packet.payload.len() as u64);

    Ok(())
}
