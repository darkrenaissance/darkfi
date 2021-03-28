use async_executor::Executor;
use async_std::sync::Mutex;
use log::*;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;

use crate::net::error::{NetError, NetResult};
use crate::net::messages::Message;
use crate::net::sessions::{InboundSession, OutboundSession, SeedSession};
use crate::net::{Channel, ChannelPtr, Hosts, HostsPtr, Settings, SettingsPtr};
use crate::system::{Subscriber, SubscriberPtr, Subscription};

/// List of channels that are awaiting connection.
pub type PendingChannels = Mutex<HashSet<SocketAddr>>;
/// List of connected channels.
pub type ConnectedChannels<T> = Mutex<HashMap<SocketAddr, Arc<T>>>;
/// Atomic pointer to p2p interface.
pub type P2pPtr = Arc<P2p>;

/// Top level peer-to-peer networking interface.
pub struct P2p {
    pending: PendingChannels,
    channels: ConnectedChannels<Channel>,
    channel_subscriber: SubscriberPtr<NetResult<ChannelPtr>>,
    // Used both internally and externally
    stop_subscriber: SubscriberPtr<NetError>,
    hosts: HostsPtr,
    settings: SettingsPtr,
}

impl P2p {
    /// Create a new p2p network.
    pub fn new(settings: Settings) -> Arc<Self> {
        let settings = Arc::new(settings);
        Arc::new(Self {
            pending: Mutex::new(HashSet::new()),
            channels: Mutex::new(HashMap::new()),
            channel_subscriber: Subscriber::new(),
            stop_subscriber: Subscriber::new(),
            hosts: Hosts::new(),
            settings,
        })
    }

    /// Invoke startup and seeding sequence. Call from constructing thread.
    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> NetResult<()> {
        debug!(target: "net", "P2p::start() [BEGIN]");
        // Start manual connections

        // Start seed session
        let seed = SeedSession::new(Arc::downgrade(&self));
        // This will block until all seed queries have finished
        seed.start(executor.clone()).await?;

        debug!(target: "net", "P2p::start() [END]");
        Ok(())
    }

    /// Synchronize the blockchain and then begin long running sessions,
    /// call after start() is invoked.
    pub async fn run(self: Arc<Self>, executor: Arc<Executor<'_>>) -> NetResult<()> {
        debug!(target: "net", "P2p::run() [BEGIN]");

        let inbound = InboundSession::new(Arc::downgrade(&self));
        inbound.clone().start(executor.clone())?;

        let outbound = OutboundSession::new(Arc::downgrade(&self));
        outbound.clone().start(executor.clone()).await?;

        let stop_sub = self.subscribe_stop().await;
        // Wait for stop signal
        stop_sub.receive().await;

        // Stop the sessions
        inbound.stop().await;
        outbound.stop().await;

        debug!(target: "net", "P2p::run() [BEGIN]");
        Ok(())
    }

    /// Broadcasts a message across all channels.
    pub async fn broadcast<M: Message + Clone>(&self, message: M) -> NetResult<()> {
        for channel in self.channels.lock().await.values() {
            channel.send(message.clone()).await?;
        }
        Ok(())
    }

    /// Add channel address to the list of connected channels.
    pub async fn store(&self, channel: ChannelPtr) {
        self.channels
            .lock()
            .await
            .insert(channel.address(), channel.clone());
        self.channel_subscriber.notify(Ok(channel)).await;
    }

    /// Remove a channel from the list of connected channels.
    pub async fn remove(&self, channel: ChannelPtr) {
        self.channels.lock().await.remove(&channel.address());
    }

    /// Check whether a channel is stored in the list of connected channels.
    pub async fn exists(&self, addr: &SocketAddr) -> bool {
        self.channels.lock().await.contains_key(addr)
    }

    /// Add a channel to the list of pending channels.
    pub async fn add_pending(&self, addr: SocketAddr) -> bool {
        self.pending.lock().await.insert(addr)
    }

    /// Remove a channel from the list of pending channels.
    pub async fn remove_pending(&self, addr: &SocketAddr) {
        self.pending.lock().await.remove(addr);
    }

    /// Return the number of connected channels.
    pub async fn connections_count(&self) -> usize {
        self.channels.lock().await.len()
    }

    /// Return an atomic pointer to the default network settings.
    pub fn settings(&self) -> SettingsPtr {
        self.settings.clone()
    }

    /// Return an atomic pointer to the list of hosts.
    pub fn hosts(&self) -> HostsPtr {
        self.hosts.clone()
    }

    /// Subscribe to a channel.
    pub async fn subscribe_channel(&self) -> Subscription<NetResult<ChannelPtr>> {
        self.channel_subscriber.clone().subscribe().await
    }

    /// Stop a subscription.
    pub async fn subscribe_stop(&self) -> Subscription<NetError> {
        self.stop_subscriber.clone().subscribe().await
    }
}
