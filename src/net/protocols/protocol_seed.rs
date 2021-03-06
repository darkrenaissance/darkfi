use log::*;
use smol::Executor;
use std::sync::Arc;

use crate::net::error::NetResult;
use crate::net::messages;
use crate::net::{ChannelPtr, HostsPtr, SettingsPtr};

pub struct ProtocolSeed {
    channel: ChannelPtr,
    hosts: HostsPtr,
    settings: SettingsPtr,
}

impl ProtocolSeed {
    pub fn new(channel: ChannelPtr, hosts: HostsPtr, settings: SettingsPtr) -> Arc<Self> {
        Arc::new(Self {
            channel,
            hosts,
            settings,
        })
    }

    pub async fn start(self: Arc<Self>, _executor: Arc<Executor<'_>>) -> NetResult<()> {
        debug!(target: "net", "ProtocolSeed::start() [START]");
        let addr_sub = self
            .channel
            .clone()
            .subscribe_msg::<messages::AddrsMessage>()
            .await
            .expect("Missing addrs dispatcher!");

        // Send own address to the seed server
        self.send_own_address().await?;

        // Send get address message
        let get_addr = messages::Message::GetAddrs(messages::GetAddrsMessage {});
        self.channel.clone().send(get_addr).await?;

        // Receive addresses
        let addrs_msg = addr_sub.receive().await?;
        self.hosts.store(addrs_msg.addrs.clone()).await;

        debug!(target: "net", "ProtocolSeed::start() [END]");
        Ok(())
    }

    pub async fn send_own_address(&self) -> NetResult<()> {
        match self.settings.external_addr {
            Some(addr) => {
                let addr = messages::Message::Addrs(messages::AddrsMessage { addrs: vec![addr] });
                self.channel.clone().send(addr).await?;
            }
            None => {
                // Do nothing if external address is not configured
            }
        }
        Ok(())
    }
}
