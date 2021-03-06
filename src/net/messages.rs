use futures::prelude::*;
use log::*;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use smol::Executor;
use smol::Timer;
use std::convert::TryFrom;
use std::io;
use std::io::Cursor;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use crate::async_serial::{AsyncReadExt, AsyncWriteExt};
use crate::error::{Error, Result};
pub use crate::net::AsyncTcpStream;
use crate::serial::{serialize, Decodable, Encodable, VarInt};

const MAGIC_BYTES: [u8; 4] = [0xd9, 0xef, 0xb6, 0x7d];

pub struct PingMessage {
    pub nonce: u32,
}

pub struct PongMessage {
    pub nonce: u32,
}

pub struct GetAddrsMessage {}

pub struct AddrsMessage {
    pub addrs: Vec<SocketAddr>,
}

pub struct VersionMessage {}

pub struct VerackMessage {}

impl Encodable for PingMessage {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.nonce.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for PingMessage {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            nonce: Decodable::decode(&mut d)?,
        })
    }
}

impl Encodable for PongMessage {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.nonce.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for PongMessage {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            nonce: Decodable::decode(&mut d)?,
        })
    }
}

impl Encodable for GetAddrsMessage {
    fn encode<S: io::Write>(&self, mut _s: S) -> Result<usize> {
        let len = 0;
        Ok(len)
    }
}

impl Decodable for GetAddrsMessage {
    fn decode<D: io::Read>(mut _d: D) -> Result<Self> {
        Ok(Self {})
    }
}

impl Encodable for AddrsMessage {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.addrs.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for AddrsMessage {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            addrs: Decodable::decode(&mut d)?,
        })
    }
}

impl Encodable for VersionMessage {
    fn encode<S: io::Write>(&self, _s: S) -> Result<usize> {
        Ok(0)
    }
}

impl Decodable for VersionMessage {
    fn decode<D: io::Read>(_d: D) -> Result<Self> {
        Ok(Self {})
    }
}

impl Encodable for VerackMessage {
    fn encode<S: io::Write>(&self, _s: S) -> Result<usize> {
        Ok(0)
    }
}

impl Decodable for VerackMessage {
    fn decode<D: io::Read>(_d: D) -> Result<Self> {
        Ok(Self {})
    }
}

// Packets are the base type read from the network
// These are converted to messages and passed to event loop
pub struct Packet {
    pub command: String,
    pub payload: Vec<u8>,
}

pub async fn read_packet<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Packet> {
    // Packets have a 4 byte header of magic digits
    // This is used for network debugging
    let mut magic = [0u8; 4];
    debug!(target: "net", "reading magic...");
    stream.read_exact(&mut magic).await?;
    debug!(target: "net", "read magic {:?}", magic);
    if magic != MAGIC_BYTES {
        return Err(Error::MalformedPacket);
    }

    // The type of the message
    let command_len = VarInt::decode_async(stream).await?.0 as usize;
    let mut command = vec![0u8; command_len];
    if command_len > 0 {
        stream.read_exact(&mut command).await?;
    }
    let command = String::from_utf8(command)?;
    debug!(target: "net", "read command: {}", command);

    let payload_len = VarInt::decode_async(stream).await?.0 as usize;

    // The message-dependent data (see message types)
    let mut payload = vec![0u8; payload_len];
    if payload_len > 0 {
        stream.read_exact(&mut payload).await?;
    }
    debug!(target: "net", "read payload {} bytes", payload_len);

    Ok(Packet { command: command, payload })
}

pub async fn send_packet<W: AsyncWrite + Unpin>(stream: &mut W, packet: Packet) -> Result<()> {
    debug!(target: "net", "sending magic...");
    stream.write_all(&MAGIC_BYTES).await?;
    debug!(target: "net", "sent magic...");

    VarInt(packet.command.len() as u64)
        .encode_async(stream)
        .await?;
    assert!(!packet.command.is_empty());
    stream.write_all(&packet.command.as_bytes()).await?;
    debug!(target: "net", "sent command: {}", packet.command);

    assert_eq!(std::mem::size_of::<usize>(), std::mem::size_of::<u64>());
    VarInt(packet.payload.len() as u64)
        .encode_async(stream)
        .await?;

    if packet.payload.len() > 0 {
        stream.write_all(&packet.payload).await?;
    }
    debug!(target: "net", "sent payload {} bytes", packet.payload.len() as u64);

    Ok(())
}

