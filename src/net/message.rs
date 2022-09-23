use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use log::debug;
use url::Url;

use crate::{
    util::serial::{Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
    Error, Result,
};

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
    debug!(target: "net", "reading magic...");

    stream.read_exact(&mut magic).await?;

    debug!(target: "net", "read magic {:?}", magic);
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
    debug!(target: "net", "read command: {}", cmd);

    let payload_len = VarInt::decode_async(stream).await?.0 as usize;

    // The message-dependent data (see message types)
    let mut payload = vec![0u8; payload_len];
    if payload_len > 0 {
        stream.read_exact(&mut payload).await?;
    }
    debug!(target: "net", "read payload {} bytes", payload_len);

    Ok(Packet { command: cmd, payload })
}

/// Sends an outbound packet by writing data to TCP stream.
pub async fn send_packet<W: AsyncWrite + Unpin + Sized>(
    stream: &mut W,
    packet: Packet,
) -> Result<()> {
    debug!(target: "net", "sending magic...");
    stream.write_all(&MAGIC_BYTES).await?;
    debug!(target: "net", "sent magic...");

    VarInt(packet.command.len() as u64).encode_async(stream).await?;
    assert!(!packet.command.is_empty());
    stream.write_all(packet.command.as_bytes()).await?;
    debug!(target: "net", "sent command: {}", packet.command);

    assert_eq!(std::mem::size_of::<usize>(), std::mem::size_of::<u64>());
    VarInt(packet.payload.len() as u64).encode_async(stream).await?;

    if !packet.payload.is_empty() {
        stream.write_all(&packet.payload).await?;
    }
    debug!(target: "net", "sent payload {} bytes", packet.payload.len() as u64);

    Ok(())
}
