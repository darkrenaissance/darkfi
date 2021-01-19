use futures::FutureExt;
use smol::Executor;
use std::sync::Arc;
use owning_ref::OwningRef;

use crate::error::{Error, Result};
use crate::net::{ChannelPtr, HostsPtr, SettingsPtr};
use crate::net::messages;

pub struct ProtocolSeed {
    channel: ChannelPtr,
    hosts: HostsPtr,
    settings: SettingsPtr,
}

impl ProtocolSeed {
    pub fn new(channel: ChannelPtr, hosts: HostsPtr, settings: SettingsPtr) -> Arc<Self> {
        Arc::new(Self { channel, hosts, settings })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        let addr_sub = self.channel.clone().subscribe_msg(messages::PacketType::Addrs).await;

        // Send own address to the seed server
        self.send_own_address().await?;

        // Send get address message
        let get_addr = messages::Message::GetAddrs(messages::GetAddrsMessage {});
        self.channel.clone().send(get_addr).await?;

        // Receive addresses
        let addrs_msg = receive_message!(addr_sub, messages::Message::Addrs);
        self.hosts.store(addrs_msg.addrs.clone()).await;

        Ok(())
    }

    pub async fn send_own_address(&self) -> Result<()> {
        match self.settings.external_addr {
            Some(addr) => {
                let addr = messages::Message::Addrs(messages::AddrsMessage { addrs: vec![addr] });
                self.channel.clone().send(addr).await?;
            },
            None => {
                // Do nothing if external address is not configured
            }
        }
        Ok(())
    }
}

