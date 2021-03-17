use log::*;
use smol::Executor;
use std::sync::Arc;

use crate::net::error::NetResult;
use crate::net::message_subscriber::MessageSubscription;
use crate::net::messages;
use crate::net::protocols::{ProtocolJobsManager, ProtocolJobsManagerPtr};
use crate::net::{ChannelPtr, HostsPtr};

pub struct ProtocolAddress {
    channel: ChannelPtr,

    addrs_sub: MessageSubscription<messages::AddrsMessage>,
    get_addrs_sub: MessageSubscription<messages::GetAddrsMessage>,

    hosts: HostsPtr,

    jobsman: ProtocolJobsManagerPtr,
}

impl ProtocolAddress {
    pub async fn new(channel: ChannelPtr, hosts: HostsPtr) -> Arc<Self> {
        let addrs_sub = channel
            .clone()
            .subscribe_msg::<messages::AddrsMessage>()
            .await
            .expect("Missing addrs dispatcher!");

        let get_addrs_sub = channel
            .clone()
            .subscribe_msg::<messages::GetAddrsMessage>()
            .await
            .expect("Missing getaddrs dispatcher!");

        Arc::new(Self {
            channel: channel.clone(),
            addrs_sub,
            get_addrs_sub,
            hosts,
            jobsman: ProtocolJobsManager::new("ProtocolAddress", channel),
        })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) {
        debug!(target: "net", "ProtocolAddress::start() [START]");
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
        let get_addrs = messages::GetAddrsMessage {};
        let _ = self.channel.clone().send(get_addrs).await;
        debug!(target: "net", "ProtocolAddress::start() [END]");
    }

    async fn handle_receive_addrs(self: Arc<Self>) -> NetResult<()> {
        debug!(target: "net", "ProtocolAddress::handle_receive_addrs() [START]");
        loop {
            let addrs_msg = self.addrs_sub.receive().await?;

            debug!(
                target: "net",
                "ProtocolAddress::handle_receive_addrs() received {} addrs",
                addrs_msg.addrs.len()
            );
            for (i, addr) in addrs_msg.addrs.iter().enumerate() {
                debug!("  addr[{}]: {}", i, addr);
            }
            self.hosts.store(addrs_msg.addrs.clone()).await;
        }
    }

    async fn handle_receive_get_addrs(self: Arc<Self>) -> NetResult<()> {
        debug!(target: "net", "ProtocolAddress::handle_receive_get_addrs() [START]");
        loop {
            let _get_addrs = self.get_addrs_sub.receive().await?;

            debug!(target: "net", "ProtocolAddress::handle_receive_get_addrs() received GetAddrs message");

            let addrs = self.hosts.load_all().await;
            debug!(
                target: "net",
                "ProtocolAddress::handle_receive_get_addrs() sending {} addrs",
                addrs.len()
            );
            let addrs_msg = messages::AddrsMessage { addrs };
            self.channel.clone().send(addrs_msg).await?;
        }
    }
}
