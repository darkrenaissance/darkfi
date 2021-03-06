use async_executor::Executor;
use async_std::sync::Mutex;
use log::*;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;

use crate::net::error::{NetError, NetResult};
use crate::net::sessions::{InboundSession, OutboundSession, SeedSession};
use crate::net::{Channel, ChannelPtr, Hosts, HostsPtr, Settings, SettingsPtr};
use crate::system::{Subscriber, SubscriberPtr, Subscription};

pub type PendingChannels = Mutex<HashSet<SocketAddr>>;
pub type ConnectedChannels<T> = Mutex<HashMap<SocketAddr, Arc<T>>>;

pub type P2pPtr = Arc<P2p>;

pub struct P2p {
    pending: PendingChannels,
    channels: ConnectedChannels<Channel>,
    // Used internally
    stop_subscriber: SubscriberPtr<NetError>,
    hosts: HostsPtr,
    settings: SettingsPtr,
}

impl P2p {
    pub fn new(settings: Settings) -> Arc<Self> {
        let settings = Arc::new(settings);
        Arc::new(Self {
            pending: Mutex::new(HashSet::new()),
            channels: Mutex::new(HashMap::new()),
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

    pub async fn store(&self, channel: ChannelPtr) {
        self.channels
            .lock()
            .await
            .insert(channel.address(), channel);
    }
    pub async fn remove(&self, channel: ChannelPtr) {
        self.channels.lock().await.remove(&channel.address());
    }

    pub async fn exists(&self, addr: &SocketAddr) -> bool {
        self.channels.lock().await.contains_key(addr)
    }

    pub async fn add_pending(&self, addr: SocketAddr) -> bool {
        self.pending.lock().await.insert(addr)
    }
    pub async fn remove_pending(&self, addr: &SocketAddr) {
        self.pending.lock().await.remove(addr);
    }

    pub async fn connections_count(&self) -> usize {
        self.channels.lock().await.len()
    }

    pub fn settings(&self) -> SettingsPtr {
        self.settings.clone()
    }

    pub fn hosts(&self) -> HostsPtr {
        self.hosts.clone()
    }

    async fn subscribe_stop(&self) -> Subscription<NetError> {
        self.stop_subscriber.clone().subscribe().await
    }
}
