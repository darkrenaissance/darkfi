use log::*;
use smol::Executor;
use std::sync::Arc;

use crate::error::Result;
use crate::net::message_subscriber::MessageSubscription;
use crate::net::messages;
use crate::net::protocols::{ProtocolJobsManager, ProtocolJobsManagerPtr};
use crate::net::{ChannelPtr, HostsPtr};

/// Defines address and get-address messages.
pub struct ProtocolAddress {
    channel: ChannelPtr,
    addrs_sub: MessageSubscription<messages::AddrsMessage>,
    get_addrs_sub: MessageSubscription<messages::GetAddrsMessage>,
    hosts: HostsPtr,
    jobsman: ProtocolJobsManagerPtr,
}

impl ProtocolAddress {
    /// Create a new address protocol. Makes an address and get-address
    /// subscription and adds them to the address protocol instance.
    pub async fn new(channel: ChannelPtr, hosts: HostsPtr) -> Arc<Self> {
        // Creates a subscription to address message.
        let addrs_sub = channel
            .clone()
            .subscribe_msg::<messages::AddrsMessage>()
            .await
            .expect("Missing addrs dispatcher!");

        // Creates a subscription to get-address message.
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

    /// Starts the address protocol. Runs receive address and get address
    /// protocols on the protocol task manager. Then sends get-address
    /// message.
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

        // Send get_address message.
        let get_addrs = messages::GetAddrsMessage {};
        let _ = self.channel.clone().send(get_addrs).await;
        debug!(target: "net", "ProtocolAddress::start() [END]");
    }

    /// Handles receiving the address message. Loops to continually recieve
    /// address messages on the address subsciption. Adds the recieved
    /// addresses to the list of hosts.
    async fn handle_receive_addrs(self: Arc<Self>) -> Result<()> {
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

    /// Handles receiving the get-address message. Continually recieves
    /// get-address messages on the get-address subsciption. Then replies
    /// with an address message.
    async fn handle_receive_get_addrs(self: Arc<Self>) -> Result<()> {
        debug!(target: "net", "ProtocolAddress::handle_receive_get_addrs() [START]");
        loop {
            let _get_addrs = self.get_addrs_sub.receive().await?;

            debug!(target: "net", "ProtocolAddress::handle_receive_get_addrs() received GetAddrs message");

            // Loads the list of hosts.
            let addrs = self.hosts.load_all().await;
            debug!(
                target: "net",
                "ProtocolAddress::handle_receive_get_addrs() sending {} addrs",
                addrs.len()
            );
            // Creates an address messages containing host address.
            let addrs_msg = messages::AddrsMessage { addrs };
            // Sends the address message across the channel.
            self.channel.clone().send(addrs_msg).await?;
        }
    }
}
