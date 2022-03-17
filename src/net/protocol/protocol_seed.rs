use async_trait::async_trait;
use log::debug;
use smol::Executor;
use std::sync::Arc;

use crate::{
    error::Result,
    net::{
        message,
        protocol::{ProtocolBase, ProtocolBasePtr},
        ChannelPtr, HostsPtr, P2pPtr, SettingsPtr,
    },
};

/// Implements the seed protocol.
pub struct ProtocolSeed {
    channel: ChannelPtr,
    hosts: HostsPtr,
    settings: SettingsPtr,
}

impl ProtocolSeed {
    /// Create a new seed protocol.
    pub async fn init(channel: ChannelPtr, p2p: P2pPtr) -> ProtocolBasePtr {
        let hosts = p2p.hosts();
        let settings = p2p.settings();

        Arc::new(Self { channel, hosts, settings })
    }

    /// Sends own external address over a channel. Imports own external address
    /// from settings, then adds that address to an address message and
    /// sends it out over the channel.
    pub async fn send_self_address(&self) -> Result<()> {
        match self.settings.external_addr {
            Some(addr) => {
                debug!(target: "net", "ProtocolSeed::send_own_address() addr={}", addr);
                let addr = message::AddrsMessage { addrs: vec![addr] };
                Ok(self.channel.clone().send(addr).await?)
            }
            // Do nothing if external address is not configured
            None => Ok(()),
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolSeed {
    /// Starts the seed protocol. Creates a subscription to the address message,
    /// then sends our address to the seed server. Sends a get-address
    /// message and receives an address message.
    async fn start(self: Arc<Self>, _executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net", "ProtocolSeed::start() [START]");
        // Create a subscription to address message.
        let addr_sub = self
            .channel
            .clone()
            .subscribe_msg::<message::AddrsMessage>()
            .await
            .expect("Missing addrs dispatcher!");

        // Send own address to the seed server.
        self.send_self_address().await?;

        // Send get address message.
        let get_addr = message::GetAddrsMessage {};
        self.channel.clone().send(get_addr).await?;

        // Receive addresses.
        let addrs_msg = addr_sub.receive().await?;
        debug!(target: "net", "ProtocolSeed::start() received {} addrs", addrs_msg.addrs.len());
        self.hosts.store(addrs_msg.addrs.clone()).await;

        debug!(target: "net", "ProtocolSeed::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolSeed"
    }
}
