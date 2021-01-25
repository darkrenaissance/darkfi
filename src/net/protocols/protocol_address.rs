use smol::Executor;
use std::sync::Arc;

use crate::net::error::NetResult;
use crate::net::messages;
use crate::net::protocols::{ProtocolJobsManager, ProtocolJobsManagerPtr};
use crate::net::{ChannelPtr, HostsPtr, SettingsPtr};

pub struct ProtocolAddress {
    channel: ChannelPtr,
    hosts: HostsPtr,
    settings: SettingsPtr,

    jobsman: ProtocolJobsManagerPtr,
}

impl ProtocolAddress {
    pub fn new(channel: ChannelPtr, hosts: HostsPtr, settings: SettingsPtr) -> Arc<Self> {
        Arc::new(Self {
            channel: channel.clone(),
            hosts,
            settings,
            jobsman: ProtocolJobsManager::new(channel),
        })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) {
        self.jobsman.clone().start(executor.clone());
        self.jobsman
            .clone()
            .spawn(self.clone().handle_receive_addrs(), executor.clone())
            .await;
        self.jobsman
            .clone()
            .spawn(self.clone().handle_receive_get_addrs(), executor)
            .await;

        // Send get_address message
        let get_addrs = messages::Message::GetAddrs(messages::GetAddrsMessage {});
        let _ = self.channel.clone().send(get_addrs).await;
    }

    async fn handle_receive_addrs(self: Arc<Self>) -> NetResult<()> {
        let addrs_sub = self
            .channel
            .clone()
            .subscribe_msg(messages::PacketType::Addrs)
            .await;

        loop {
            let addrs_msg = receive_message!(addrs_sub, messages::Message::Addrs);

            self.hosts.store(addrs_msg.addrs.clone()).await;
        }
    }

    async fn handle_receive_get_addrs(self: Arc<Self>) -> NetResult<()> {
        let get_addrs_sub = self
            .channel
            .clone()
            .subscribe_msg(messages::PacketType::GetAddrs)
            .await;

        loop {
            let _get_addrs = receive_message!(get_addrs_sub, messages::Message::GetAddrs);

            let addrs = messages::Message::Addrs(messages::AddrsMessage {
                addrs: self.hosts.load_all().await,
            });
            self.channel.clone().send(addrs).await?;
        }
    }
}
