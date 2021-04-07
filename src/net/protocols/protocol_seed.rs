use log::*;
use smol::Executor;
use std::sync::Arc;

use crate::net::error::NetResult;
use crate::net::messages;
use crate::net::{ChannelPtr, HostsPtr, SettingsPtr};

/// Implements the seed protocol.
pub struct ProtocolSeed {
    channel: ChannelPtr,
    hosts: HostsPtr,
    settings: SettingsPtr,
}

impl ProtocolSeed {
    /// Create a new seed protocol.
    pub fn new(channel: ChannelPtr, hosts: HostsPtr, settings: SettingsPtr) -> Arc<Self> {
        Arc::new(Self {
            channel,
            hosts,
            settings,
        })
    }

    /// Starts the seed protocol. Creates a subscription to the address message,
    /// then sends our address to the seed server. Sends a get-address
    /// message and receives an address message.
    pub async fn start(self: Arc<Self>, _executor: Arc<Executor<'_>>) -> NetResult<()> {
        debug!(target: "net", "ProtocolSeed::start() [START]");
        // Create a subscription to address message.
        let addr_sub = self
            .channel
            .clone()
            .subscribe_msg::<messages::AddrsMessage>()
            .await
            .expect("Missing addrs dispatcher!");

        // Send own address to the seed server.
        self.send_self_address().await?;

        // Send get address message.
        let get_addr = messages::GetAddrsMessage {};
        self.channel.clone().send(get_addr).await?;

        // Receive addresses.
        let addrs_msg = addr_sub.receive().await?;
        debug!(target: "net", "ProtocolSeed::start() received {} addrs", addrs_msg.addrs.len());
        self.hosts.store(addrs_msg.addrs.clone()).await;

        debug!(target: "net", "ProtocolSeed::start() [END]");
        Ok(())
    }

    /// Sends own external address over a channel. Imports own external address
    /// from settings, then adds that address to an address message and
    /// sends it out over the channel.
    pub async fn send_self_address(&self) -> NetResult<()> {
        match self.settings.external_addr {
            Some(addr) => {
                debug!(target: "net", "ProtocolSeed::send_own_address() addr={}", addr);
                let addr = messages::AddrsMessage { addrs: vec![addr] };
                self.channel.clone().send(addr).await?;
            }
            None => {
                // Do nothing if external address is not configured
            }
        }
        Ok(())
    }
}
