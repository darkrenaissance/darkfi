use async_std::sync::{Arc, Mutex};
use std::fmt;

use async_executor::Executor;
use fxhash::{FxHashMap, FxHashSet};
use log::debug;
use serde_json::json;
use url::Url;

use crate::{
    system::{Subscriber, SubscriberPtr, Subscription},
    Error, Result,
};

use super::{
    message::Message,
    protocol::{register_default_protocols, ProtocolRegistry},
    session::{InboundSession, ManualSession, OutboundSession, SeedSession, Session},
    Channel, ChannelPtr, Hosts, HostsPtr, Settings, SettingsPtr,
};

/// List of channels that are awaiting connection.
pub type PendingChannels = Mutex<FxHashSet<Url>>;
/// List of connected channels.
pub type ConnectedChannels = Mutex<fxhash::FxHashMap<Url, Arc<Channel>>>;
/// Atomic pointer to p2p interface.
pub type P2pPtr = Arc<P2p>;

enum P2pState {
    // The p2p object has been created but not yet started.
    Open,
    // We are performing the initial seed session
    Start,
    // Seed session finished, but not yet running
    Started,
    // p2p is running and the network is active.
    Run,
}

impl fmt::Display for P2pState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Open => "open",
                Self::Start => "start",
                Self::Started => "started",
                Self::Run => "run",
            }
        )
    }
}

/// Top level peer-to-peer networking interface.
pub struct P2p {
    pending: PendingChannels,
    channels: ConnectedChannels,
    channel_subscriber: SubscriberPtr<Result<ChannelPtr>>,
    // Used both internally and externally
    stop_subscriber: SubscriberPtr<Error>,
    hosts: HostsPtr,
    protocol_registry: ProtocolRegistry,

    // We keep a reference to the sessions used for get info
    session_manual: Mutex<Option<Arc<ManualSession>>>,
    session_inbound: Mutex<Option<Arc<InboundSession>>>,
    session_outbound: Mutex<Option<Arc<OutboundSession>>>,

    state: Mutex<P2pState>,

    settings: SettingsPtr,
}

impl P2p {
    // TODO: documentation is unclear
    /// Create a new p2p network.
    pub async fn new(settings: Settings) -> Arc<Self> {
        let settings = Arc::new(settings);

        let self_ = Arc::new(Self {
            pending: Mutex::new(FxHashSet::default()),
            channels: Mutex::new(FxHashMap::default()),
            channel_subscriber: Subscriber::new(),
            stop_subscriber: Subscriber::new(),
            hosts: Hosts::new(),
            protocol_registry: ProtocolRegistry::new(),
            session_manual: Mutex::new(None),
            session_inbound: Mutex::new(None),
            session_outbound: Mutex::new(None),
            state: Mutex::new(P2pState::Open),
            settings,
        });

        let parent = Arc::downgrade(&self_);

        *self_.session_manual.lock().await = Some(ManualSession::new(parent.clone()));
        *self_.session_inbound.lock().await = Some(InboundSession::new(parent.clone()).await);
        *self_.session_outbound.lock().await = Some(OutboundSession::new(parent));

        register_default_protocols(self_.clone()).await;

        self_
    }

    pub async fn get_info(&self) -> serde_json::Value {
        let external_addr = self
            .settings
            .external_addr
            .as_ref()
            .map(|addr| serde_json::Value::from(addr.to_string()))
            .unwrap_or(serde_json::Value::Null);

        json!({
            "external_addr": external_addr,
            "session_manual": self.session_manual().await.get_info().await,
            "session_inbound": self.session_inbound().await.get_info().await,
            "session_outbound": self.session_outbound().await.get_info().await,
            "state": self.state.lock().await.to_string(),
        })
    }

    /// Invoke startup and seeding sequence. Call from constructing thread.
    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net", "P2p::start() [BEGIN]");

        *self.state.lock().await = P2pState::Start;

        // Start seed session
        let seed = SeedSession::new(Arc::downgrade(&self));
        // This will block until all seed queries have finished
        seed.start(executor.clone()).await?;

        *self.state.lock().await = P2pState::Started;

        debug!(target: "net", "P2p::start() [END]");
        Ok(())
    }

    pub async fn session_manual(&self) -> Arc<ManualSession> {
        self.session_manual.lock().await.as_ref().unwrap().clone()
    }
    pub async fn session_inbound(&self) -> Arc<InboundSession> {
        self.session_inbound.lock().await.as_ref().unwrap().clone()
    }
    pub async fn session_outbound(&self) -> Arc<OutboundSession> {
        self.session_outbound.lock().await.as_ref().unwrap().clone()
    }

    // TODO: this documentation is wrong
    /// Synchronize the blockchain and then begin long running sessions,
    /// call after start() is invoked.
    pub async fn run(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net", "P2p::run() [BEGIN]");

        *self.state.lock().await = P2pState::Run;

        let manual = self.session_manual().await;
        for peer in &self.settings.peers {
            manual.clone().connect(peer, executor.clone()).await;
        }

        let inbound = self.session_inbound().await;
        inbound.clone().start(executor.clone()).await?;

        let outbound = self.session_outbound().await;
        outbound.clone().start(executor.clone()).await?;

        let stop_sub = self.subscribe_stop().await;
        // Wait for stop signal
        stop_sub.receive().await;

        // Stop the sessions
        manual.stop().await;
        inbound.stop().await;
        outbound.stop().await;

        debug!(target: "net", "P2p::run() [END]");
        Ok(())
    }

    /// Broadcasts a message across all channels.
    pub async fn broadcast<M: Message + Clone>(&self, message: M) -> Result<()> {
        for channel in self.channels.lock().await.values() {
            channel.send(message.clone()).await?;
        }
        Ok(())
    }

    /// Broadcasts a message across all channels.
    /// exclude channels provided in exclude_list
    pub async fn broadcast_with_exclude<M: Message + Clone>(
        &self,
        message: M,
        exclude_list: &[Url],
    ) -> Result<()> {
        for channel in self.channels.lock().await.values() {
            if exclude_list.contains(&channel.address()) {
                continue
            }
            channel.send(message.clone()).await?;
        }
        Ok(())
    }

    /// Add channel address to the list of connected channels.
    pub async fn store(&self, channel: ChannelPtr) {
        self.channels.lock().await.insert(channel.address(), channel.clone());
        self.channel_subscriber.notify(Ok(channel)).await;
    }

    /// Remove a channel from the list of connected channels.
    pub async fn remove(&self, channel: ChannelPtr) {
        self.channels.lock().await.remove(&channel.address());
    }

    /// Check whether a channel is stored in the list of connected channels.
    pub async fn exists(&self, addr: &Url) -> bool {
        self.channels.lock().await.contains_key(addr)
    }

    /// Add a channel to the list of pending channels.
    pub async fn add_pending(&self, addr: Url) -> bool {
        self.pending.lock().await.insert(addr)
    }

    /// Remove a channel from the list of pending channels.
    pub async fn remove_pending(&self, addr: &Url) {
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

    pub fn protocol_registry(&self) -> &ProtocolRegistry {
        &self.protocol_registry
    }

    /// Subscribe to a channel.
    pub async fn subscribe_channel(&self) -> Subscription<Result<ChannelPtr>> {
        self.channel_subscriber.clone().subscribe().await
    }

    /// Subscribe to a stop signal.
    pub async fn subscribe_stop(&self) -> Subscription<Error> {
        self.stop_subscriber.clone().subscribe().await
    }

    pub fn channels(&self) -> &ConnectedChannels {
        &self.channels
    }
}
