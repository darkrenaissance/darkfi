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

pub type Ciphertext = Vec<u8>;
pub type CiphertextHash = [u8; 32];

// Packets and Message because Rust doesn't allow value
// aliasing from ADL type enums (which Message uses).
#[derive(IntoPrimitive, TryFromPrimitive, Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum PacketType {
    Ping = 1,
    Pong = 2,
    GetAddrs = 3,
    Addrs = 4,
    Inv = 5,
    GetSlabs = 6,
    Slab = 7,
    Version = 8,
    Verack = 9,
}

pub enum Message {
    Ping(PingMessage),
    Pong(PongMessage),
    GetAddrs(GetAddrsMessage),
    Addrs(AddrsMessage),
    Inv(InvMessage),
    GetSlabs(GetSlabsMessage),
    Slab(SlabMessage),
    Version(VersionMessage),
    Verack(VerackMessage),
}

pub struct PingMessage {
    pub nonce: u32,
}

pub struct PongMessage {
    pub nonce: u32,
}

pub struct GetAddrsMessage {}

pub struct GetSlabsMessage {
    pub slabs_hash: Vec<[u8; 32]>,
}

#[derive(Clone)]
pub struct SlabMessage {
    pub nonce: [u8; 12],
    pub ciphertext: Ciphertext,
}

pub struct InvMessage {
    pub slabs_hash: Vec<[u8; 32]>,
}

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

impl Encodable for GetSlabsMessage {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.slabs_hash.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for GetSlabsMessage {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            slabs_hash: Decodable::decode(&mut d)?,
        })
    }
}

impl Encodable for SlabMessage {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.nonce.encode(&mut s)?;
        len += self.ciphertext.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for SlabMessage {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            nonce: Decodable::decode(&mut d)?,
            ciphertext: Decodable::decode(&mut d)?,
        })
    }
}

impl Encodable for InvMessage {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.slabs_hash.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for InvMessage {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            slabs_hash: Decodable::decode(&mut d)?,
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

impl Message {
    pub fn packet_type(&self) -> PacketType {
        match self {
            Message::Ping(_message) => PacketType::Ping,
            Message::Pong(_message) => PacketType::Pong,
            Message::GetAddrs(_message) => PacketType::GetAddrs,
            Message::Addrs(_message) => PacketType::Addrs,
            Message::Inv(_message) => PacketType::Inv,
            Message::GetSlabs(_message) => PacketType::GetSlabs,
            Message::Slab(_message) => PacketType::Slab,
            Message::Version(_message) => PacketType::Version,
            Message::Verack(_message) => PacketType::Verack,
        }
    }

    pub fn pack(&self) -> Result<Packet> {
        match self {
            Message::Ping(message) => {
                let mut payload = Vec::new();
                message.encode(&mut payload)?;
                Ok(Packet {
                    command: PacketType::Ping,
                    payload,
                })
            }
            Message::Pong(message) => {
                let mut payload = Vec::new();
                message.encode(&mut payload)?;
                Ok(Packet {
                    command: PacketType::Pong,
                    payload,
                })
            }
            Message::GetAddrs(message) => {
                let mut payload = Vec::new();
                message.encode(&mut payload)?;
                Ok(Packet {
                    command: PacketType::GetAddrs,
                    payload,
                })
            }
            Message::Addrs(message) => {
                let mut payload = Vec::new();
                message.encode(Cursor::new(&mut payload))?;
                Ok(Packet {
                    command: PacketType::Addrs,
                    payload,
                })
            }
            Message::Inv(message) => {
                let payload = serialize(message);
                Ok(Packet {
                    command: PacketType::Inv,
                    payload,
                })
            }
            Message::GetSlabs(message) => {
                let payload = serialize(message);
                Ok(Packet {
                    command: PacketType::GetSlabs,
                    payload,
                })
            }
            Message::Slab(message) => {
                let payload = serialize(message);
                Ok(Packet {
                    command: PacketType::Slab,
                    payload,
                })
            }
            Message::Version(message) => {
                let payload = serialize(message);
                Ok(Packet {
                    command: PacketType::Version,
                    payload,
                })
            }
            Message::Verack(message) => {
                let payload = serialize(message);
                Ok(Packet {
                    command: PacketType::Verack,
                    payload,
                })
            }
        }
    }

    pub fn unpack(packet: Packet) -> Result<Self> {
        let cursor = Cursor::new(packet.payload.clone());
        match packet.command {
            PacketType::Ping => Ok(Self::Ping(PingMessage::decode(cursor)?)),
            PacketType::Pong => Ok(Self::Pong(PongMessage::decode(cursor)?)),
            PacketType::GetAddrs => Ok(Self::GetAddrs(GetAddrsMessage::decode(cursor)?)),
            PacketType::Addrs => Ok(Self::Addrs(AddrsMessage::decode(cursor)?)),
            PacketType::Inv => Ok(Self::Inv(InvMessage::decode(cursor)?)),
            PacketType::GetSlabs => Ok(Self::GetSlabs(GetSlabsMessage::decode(cursor)?)),
            PacketType::Slab => Ok(Self::Slab(SlabMessage::decode(cursor)?)),
            PacketType::Version => Ok(Self::Version(VersionMessage::decode(cursor)?)),
            PacketType::Verack => Ok(Self::Verack(VerackMessage::decode(cursor)?)),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Message::Ping(_) => "Ping",
            Message::Pong(_) => "Pong",
            Message::GetAddrs(_) => "GetAddrs",
            Message::Addrs(_) => "Addrs",
            Message::Inv(_) => "Inv",
            Message::GetSlabs(_) => "GetSlabs",
            Message::Slab(_) => "Slab",
            Message::Version(_) => "Version",
            Message::Verack(_) => "Verack",
        }
    }
}

// Packets are the base type read from the network
// These are converted to messages and passed to event loop
pub struct Packet {
    pub command: PacketType,
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
    let command = AsyncReadExt::read_u8(stream).await?;
    debug!(target: "net", "read command: {}", command);
    let command = PacketType::try_from(command).map_err(|_| Error::MalformedPacket)?;

    let payload_len = VarInt::decode_async(stream).await?.0 as usize;

    // The message-dependent data (see message types)
    let mut payload = vec![0u8; payload_len];
    if payload_len > 0 {
        stream.read_exact(&mut payload).await?;
    }
    debug!(target: "net", "read payload {} bytes", payload_len);

    Ok(Packet { command, payload })
}

pub async fn send_packet<W: AsyncWrite + Unpin>(stream: &mut W, packet: Packet) -> Result<()> {
    debug!(target: "net", "sending magic...");
    stream.write_all(&MAGIC_BYTES).await?;
    debug!(target: "net", "sent magic...");

    AsyncWriteExt::write_u8(stream, packet.command as u8).await?;
    debug!(target: "net", "sent command: {}", packet.command as u8);

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

pub async fn receive_message<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Message> {
    let packet = read_packet(stream).await?;
    debug!(target: "net", "unpacking packet: {:?}", packet.command);
    let message = Message::unpack(packet)?;
    debug!(target: "net", "received Message::{}", message.name());
    Ok(message)
}

pub async fn send_message<W: AsyncWrite + Unpin>(stream: &mut W, message: Message) -> Result<()> {
    debug!(target: "net", "sending Message::{}", message.name());
    let packet = message.pack()?;
    send_packet(stream, packet).await
}

pub async fn sleep(seconds: u64) {
    Timer::after(Duration::from_secs(seconds)).await;
}

// Used for ping pong loop timer
pub struct InactivityTimer {
    reset_sender: async_channel::Sender<()>,
    timeout_receiver: async_channel::Receiver<()>,
    task: smol::Task<()>,
}

impl InactivityTimer {
    pub fn new(executor: Arc<Executor<'_>>) -> Self {
        let (reset_sender, reset_receiver) = async_channel::bounded::<()>(1);
        let (timeout_sender, timeout_receiver) = async_channel::bounded::<()>(1);

        let task = executor.spawn(async {
            match Self::_start(reset_receiver, timeout_sender).await {
                Ok(()) => {}
                Err(err) => error!("InactivityTimer fatal error {}", err),
            }
        });

        Self {
            reset_sender,
            timeout_receiver,
            task,
        }
    }

    pub async fn stop(self) {
        self.task.cancel().await;
    }

    // This loop basically waits for 10 secs. If it doesn't
    // receive a signal that something happened then it will
    // send a timeout signal. This will wakeup the main event loop
    // and the connection will be dropped.
    async fn _start(
        reset_rx: async_channel::Receiver<()>,
        timeout_sx: async_channel::Sender<()>,
    ) -> Result<()> {
        loop {
            let is_awake = futures::select! {
                _ = reset_rx.recv().fuse() => true,
                _ = sleep(10).fuse() => false
            };

            if !is_awake {
                warn!("InactivityTimer timeout");
                timeout_sx.send(()).await?;
            }
        }
    }

    pub async fn reset(&self) -> Result<()> {
        self.reset_sender.send(()).await?;
        Ok(())
    }

    pub async fn wait_for_wakeup(&self) -> Result<()> {
        Ok(self.timeout_receiver.recv().await?)
    }
}
