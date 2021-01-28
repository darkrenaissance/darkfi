use async_executor::Executor;
use async_std::sync::Mutex;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::net::error::NetResult;
use crate::net::sessions::{InboundSession, SeedSession};
use crate::net::{Channel, ChannelPtr, Hosts, HostsPtr, Settings, SettingsPtr};

pub type Pending<T> = Mutex<HashMap<SocketAddr, Arc<T>>>;

pub type P2pPtr = Arc<P2p>;

pub struct P2p {
    pending_channels: Pending<Channel>,
    hosts: HostsPtr,
    settings: SettingsPtr,
}

impl P2p {
    pub fn new(settings: Settings) -> Arc<Self> {
        let settings = Arc::new(settings);
        Arc::new(Self {
            pending_channels: Mutex::new(HashMap::new()),
            hosts: Hosts::new(settings.clone()),
            settings,
        })
    }

    /// Invoke startup and seeding sequence. Call from constructing thread.
    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> NetResult<()> {
        // Start manual connections

        // Start seed session
        let seed = SeedSession::new(Arc::downgrade(&self));
        seed.start(executor.clone()).await?;

        Ok(())
    }

    /// Synchronize the blockchain and then begin long running sessions,
    /// call after start() is invoked.
    pub async fn run(self: Arc<Self>, executor: Arc<Executor<'_>>) -> NetResult<()> {
        let inbound = InboundSession::new(Arc::downgrade(&self));
        inbound.start(executor.clone())?;
        Ok(())
    }

    pub async fn store(self: Arc<Self>, channel: ChannelPtr) {
        self.pending_channels
            .lock()
            .await
            .insert(channel.address(), channel);
    }
    pub async fn remove(self: Arc<Self>, channel: ChannelPtr) {
        self.pending_channels
            .lock()
            .await
            .remove(&channel.address());
    }

    pub async fn connections_count(&self) -> usize {
        self.pending_channels.lock().await.len()
    }

    pub fn settings(&self) -> SettingsPtr {
        self.settings.clone()
    }

    pub fn hosts(&self) -> HostsPtr {
        self.hosts.clone()
    }
}
