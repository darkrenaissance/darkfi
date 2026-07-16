/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Weak,
    },
};

use futures::{stream::FuturesUnordered, TryFutureExt};
use futures_rustls::rustls::crypto::{ring, CryptoProvider};
use parking_lot::Mutex;
use smol::{fs, lock::RwLock as AsyncRwLock, stream::StreamExt};
use tracing::{debug, error, info};
use url::Url;

use super::{
    channel::{Channel, ChannelPtr},
    dnet::DnetEvent,
    hosts::{Hosts, HostsPtr},
    message::{Message, SerializedMessage},
    protocol::{protocol_registry::ProtocolRegistry, register_default_protocols},
    session::{
        DirectSession, DirectSessionPtr, InboundSession, InboundSessionPtr, ManualSession,
        ManualSessionPtr, OutboundSession, OutboundSessionPtr, RefineSession, RefineSessionPtr,
        SeedSyncSession, SeedSyncSessionPtr, Session,
    },
    settings::Settings,
};
use crate::{
    system::{ExecutorPtr, Publisher, PublisherPtr, StoppableTask, StoppableTaskPtr, Subscription},
    util::{logger::verbose, path::expand_path},
    Error, Result,
};

#[cfg(target_family = "unix")]
use smol::fs::unix::PermissionsExt;

/// Atomic pointer to the p2p interface
pub type P2pPtr = Arc<P2p>;

/// Maximum number of detached broadcasts retained by a P2P instance.
/// Calls beyond this limit are rejected with [`Error::BroadcastLimitReached`]
/// rather than queued, keeping retained payloads and channel lists bounded.
pub const MAX_CONCURRENT_BROADCASTS: usize = 64;

struct BroadcastTaskState {
    accepting: bool,
    tasks: HashSet<StoppableTaskPtr>,
}

struct BroadcastTasks {
    state: Mutex<BroadcastTaskState>,
    rejected: AtomicUsize,
}

impl BroadcastTasks {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(BroadcastTaskState { accepting: true, tasks: HashSet::new() }),
            rejected: AtomicUsize::new(0),
        })
    }

    fn start<'a, MainFut>(
        self: &Arc<Self>,
        main: MainFut,
        ex: Arc<smol::Executor<'a>>,
    ) -> Result<()>
    where
        MainFut: Future<Output = Result<()>> + Send + 'a,
    {
        let task = StoppableTask::new();
        let mut state = self.state.lock();
        if !state.accepting {
            return Err(Error::NetworkServiceStopped)
        }
        if state.tasks.len() >= MAX_CONCURRENT_BROADCASTS {
            self.rejected.fetch_add(1, Ordering::Relaxed);
            return Err(Error::BroadcastLimitReached)
        }

        state.tasks.insert(task.clone());
        let tasks = self.clone();
        let task_ = task.clone();
        task.start(
            main,
            move |_| async move {
                tasks.state.lock().tasks.remove(&task_);
            },
            Error::DetachedTaskStopped,
            ex,
        );
        Ok(())
    }

    fn close(&self) -> Vec<StoppableTaskPtr> {
        let mut state = self.state.lock();
        state.accepting = false;
        state.tasks.iter().cloned().collect()
    }

    fn stop_all_nowait(&self) {
        for task in self.close() {
            task.stop_nowait();
        }
    }

    async fn stop_all(&self) {
        for task in self.close() {
            task.stop().await;
        }
        debug_assert!(self.state.lock().tasks.is_empty());
    }

    fn reopen(&self) {
        let mut state = self.state.lock();
        debug_assert!(state.tasks.is_empty());
        state.accepting = true;
    }

    fn active(&self) -> usize {
        self.state.lock().tasks.len()
    }

    fn rejected(&self) -> usize {
        self.rejected.load(Ordering::Relaxed)
    }
}

/// Toplevel peer-to-peer networking interface
pub struct P2p {
    /// Global multithreaded executor reference
    executor: ExecutorPtr,
    /// Known hosts (peers)
    hosts: HostsPtr,
    /// Protocol registry
    protocol_registry: ProtocolRegistry,
    /// P2P network settings
    settings: Arc<AsyncRwLock<Settings>>,
    /// Reference to configured [`ManualSession`]
    session_manual: ManualSessionPtr,
    /// Reference to configured [`InboundSession`]
    session_inbound: InboundSessionPtr,
    /// Reference to configured [`OutboundSession`]
    session_outbound: OutboundSessionPtr,
    /// Reference to configured [`RefineSession`]
    session_refine: RefineSessionPtr,
    /// Reference to configured [`SeedSyncSession`]
    session_seedsync: SeedSyncSessionPtr,
    /// Reference to configured [`DirectSession`]
    session_direct: DirectSessionPtr,
    /// Enable network debugging
    pub dnet_enabled: AtomicBool,
    /// The publisher for which we can give dnet info over
    dnet_publisher: PublisherPtr<DnetEvent>,
    /// Prevents channel registration while shutdown is in progress.
    stopping: AtomicBool,
    /// All started channels, including those still performing their handshake.
    channels: Mutex<HashMap<u32, Weak<Channel>>>,
    /// Bounded set of detached broadcast tasks owned by this P2P instance.
    broadcast_tasks: Arc<BroadcastTasks>,
}

impl P2p {
    /// Initialize a new p2p network.
    ///
    /// Initializes all sessions and protocols. Adds the protocols to the protocol
    /// registry, along with a bitflag session selector that includes or excludes
    /// sessions from seed, version, and address protocols.
    ///
    /// Creates a weak pointer to self that is used by all sessions to access the
    /// p2p parent class.
    pub async fn new(settings: Settings, executor: ExecutorPtr) -> Result<P2pPtr> {
        // Create the datastore
        if let Some(ref datastore) = settings.p2p_datastore {
            let datastore = expand_path(datastore)?;
            fs::create_dir_all(&datastore).await?;
            // Windows only has readonly so don't worry about it
            #[cfg(target_family = "unix")]
            fs::set_permissions(&datastore, PermissionsExt::from_mode(0o700)).await?;
        }

        // Register a CryptoProvider for rustls
        let _ = CryptoProvider::install_default(ring::default_provider());

        // Wrap the Settings into an Arc<RwLock>
        let settings = Arc::new(AsyncRwLock::new(settings));

        let self_ = Arc::new_cyclic(|p2p| Self {
            executor,
            hosts: Hosts::new(Arc::clone(&settings)),
            protocol_registry: ProtocolRegistry::new(),
            settings,
            session_manual: ManualSession::new(p2p.clone()),
            session_inbound: InboundSession::new(p2p.clone()),
            session_outbound: OutboundSession::new(p2p.clone()),
            session_refine: RefineSession::new(p2p.clone()),
            session_seedsync: SeedSyncSession::new(p2p.clone()),
            session_direct: DirectSession::new(p2p.clone()),
            dnet_enabled: AtomicBool::new(false),
            dnet_publisher: Publisher::new(),
            stopping: AtomicBool::new(false),
            channels: Mutex::new(HashMap::new()),
            broadcast_tasks: BroadcastTasks::new(),
        });

        register_default_protocols(self_.clone()).await;

        Ok(self_)
    }

    /// Starts inbound, outbound, and manual sessions.
    pub async fn start(self: Arc<Self>) -> Result<()> {
        self.broadcast_tasks.reopen();
        self.stopping.store(false, Ordering::SeqCst);

        debug!(target: "net::p2p::start", "P2P::start() [BEGIN] [magic_bytes={:?}]",
               self.settings.read().await.magic_bytes.0);
        info!(target: "net::p2p::start", "[P2P] Starting P2P subsystem");

        // Start the inbound session
        if let Err(err) = self.session_inbound().start().await {
            error!(target: "net::p2p::start", "Failed to start inbound session!: {err}");
            return Err(err)
        }

        // Start the manual session
        self.session_manual().start().await;

        // Start the seedsync session. Seed connections will not
        // activate yet- they wait for a call to notify().
        self.session_seedsync().start().await;

        // Start the outbound session
        self.session_outbound().start().await;

        // Start the refine session
        self.session_refine().start().await;

        // Start the direct session
        self.session_direct().start().await;

        info!(target: "net::p2p::start", "[P2P] P2P subsystem started successfully");
        Ok(())
    }

    /// Reseed the P2P network.
    pub async fn seed(self: Arc<Self>) {
        debug!(target: "net::p2p::seed", "P2P::seed() [BEGIN]");

        // Activate the seed session.
        self.session_seedsync().notify().await;

        debug!(target: "net::p2p::seed", "P2P::seed() [END]");
    }

    /// Stop the running P2P subsystem
    pub async fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);

        // Reject new broadcasts and cancel retained payloads/channel pointers
        // before stopping their channels.
        self.broadcast_tasks.stop_all().await;

        // Stop connection producers before draining established channels.
        self.session_inbound().stop().await;
        self.session_manual().stop().await;
        self.session_seedsync().stop().await;
        self.session_outbound().stop().await;
        self.session_refine().stop().await;
        self.session_direct().stop().await;

        let channels = self.tracked_channels();
        let stops = FuturesUnordered::new();
        for channel in channels {
            stops.push(async move { channel.stop().await });
        }
        stops.collect::<Vec<_>>().await;

        debug_assert!(self.tracked_channels().is_empty(), "P2P stopped with active channels");
        debug_assert!(self.hosts.channels().is_empty(), "P2P stopped with registered channels");
    }

    pub(crate) fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    pub(crate) fn track_channel(&self, channel: &ChannelPtr) -> bool {
        let mut channels = self.channels.lock();
        if self.is_stopping() {
            return false
        }

        channels.retain(|_, channel| channel.strong_count() > 0);
        channels.insert(channel.info.id, Arc::downgrade(channel));
        true
    }

    pub(crate) fn untrack_channel(&self, channel_id: u32) {
        self.channels.lock().remove(&channel_id);
    }

    fn tracked_channels(&self) -> Vec<ChannelPtr> {
        self.channels.lock().values().filter_map(Weak::upgrade).collect()
    }

    /// Broadcasts a message concurrently across all active peers.
    pub async fn broadcast<M: Message>(&self, message: &M) -> Result<()> {
        self.broadcast_with_exclude(message, &[]).await
    }

    /// Broadcasts a message concurrently across active peers, excluding
    /// the ones provided in `exclude_list`.
    pub async fn broadcast_with_exclude<M: Message>(
        &self,
        message: &M,
        exclude_list: &[Url],
    ) -> Result<()> {
        let mut channels = Vec::new();
        for channel in self.hosts().peers() {
            if exclude_list.contains(channel.address()) {
                continue
            }
            channels.push(channel);
        }
        self.broadcast_to(message, &channels).await
    }

    /// Broadcast a message concurrently to all given peers.
    ///
    /// The send runs in the background when admitted. If
    /// [`MAX_CONCURRENT_BROADCASTS`] sends are already active, this returns
    /// [`Error::BroadcastLimitReached`] without retaining the message or
    /// channel list. Calls made during shutdown return
    /// [`Error::NetworkServiceStopped`].
    pub async fn broadcast_to<M: Message>(
        &self,
        message: &M,
        channel_list: &[ChannelPtr],
    ) -> Result<()> {
        if self.is_stopping() {
            return Err(Error::NetworkServiceStopped)
        }

        if channel_list.is_empty() {
            verbose!(target: "net::p2p::broadcast", "[P2P] No connected channels found for broadcast");
            return Ok(())
        }

        // Serialize the provided message
        let message = SerializedMessage::new(message).await;

        // Keep rate-limited sends detached while bounding and tracking every
        // task so shutdown can cancel and drain them.
        let channels = channel_list.to_vec();
        self.broadcast_tasks.start(
            async move {
                broadcast_serialized_to::<M>(message, channels).await;
                Ok(())
            },
            self.executor.clone(),
        )
    }

    /// Number of broadcasts currently retained by this P2P instance.
    pub fn active_broadcasts(&self) -> usize {
        self.broadcast_tasks.active()
    }

    /// Number of broadcasts rejected because the concurrency limit was full.
    pub fn rejected_broadcasts(&self) -> usize {
        self.broadcast_tasks.rejected()
    }

    /// Check whether this node has connections to any peers. This method will
    /// not report seedsync or refinery connections.
    pub fn is_connected(&self) -> bool {
        !self.hosts().peers().is_empty()
    }

    /// The number of connected peers. This means channels which are not seed or refine.
    pub fn peers_count(&self) -> usize {
        self.hosts().peers().len()
    }

    /// Return an atomic pointer to the set network settings
    pub fn settings(&self) -> Arc<AsyncRwLock<Settings>> {
        Arc::clone(&self.settings)
    }

    /// Reload settings and apply any changes to the running P2P subsystem.
    ///
    /// Users should modify settings through the settings lock, then call this
    /// method to apply the changes:
    /// ```rust
    /// let mut settings = p2p.settings().write().await;
    /// settings.outbound_connections = new_value;
    /// drop(settings);
    /// p2p.reload().await;
    /// ```
    pub async fn reload(self: Arc<Self>) {
        self.session_manual().reload().await;
        self.session_inbound().reload().await;
        self.session_outbound().reload().await;
        self.session_refine().reload().await;
        self.session_seedsync().reload().await;
        self.session_direct().reload().await;

        debug!(target: "net::p2p::reload", "P2P settings reloaded successfully");
    }

    /// Return an atomic pointer to the list of hosts
    pub fn hosts(&self) -> HostsPtr {
        self.hosts.clone()
    }

    /// Reference the global executor
    pub fn executor(&self) -> ExecutorPtr {
        self.executor.clone()
    }

    /// Return a reference to the internal protocol registry
    pub fn protocol_registry(&self) -> &ProtocolRegistry {
        &self.protocol_registry
    }

    /// Get pointer to manual session
    pub fn session_manual(&self) -> ManualSessionPtr {
        self.session_manual.clone()
    }

    /// Get pointer to inbound session
    pub fn session_inbound(&self) -> InboundSessionPtr {
        self.session_inbound.clone()
    }

    /// Get pointer to outbound session
    pub fn session_outbound(&self) -> OutboundSessionPtr {
        self.session_outbound.clone()
    }

    /// Get pointer to refine session
    pub fn session_refine(&self) -> RefineSessionPtr {
        self.session_refine.clone()
    }

    /// Get pointer to seedsync session
    pub fn session_seedsync(&self) -> SeedSyncSessionPtr {
        self.session_seedsync.clone()
    }

    /// Get pointer to direct session
    pub fn session_direct(&self) -> DirectSessionPtr {
        self.session_direct.clone()
    }

    /// Enable network debugging
    pub fn dnet_enable(&self) {
        self.dnet_enabled.store(true, Ordering::SeqCst);
        verbose!("[P2P] Network debugging enabled!");
    }

    /// Disable network debugging
    pub fn dnet_disable(&self) {
        self.dnet_enabled.store(false, Ordering::SeqCst);
        verbose!("[P2P] Network debugging disabled!");
    }

    /// Subscribe to dnet events
    pub async fn dnet_subscribe(&self) -> Subscription<DnetEvent> {
        self.dnet_publisher.clone().subscribe().await
    }

    /// Send a dnet notification over the publisher
    pub(super) async fn dnet_notify(&self, event: DnetEvent) {
        self.dnet_publisher.notify(event).await;
    }

    /// Grab the channel pointer of provided channel ID, if it exists.
    pub fn get_channel(&self, id: u32) -> Option<ChannelPtr> {
        self.hosts.get_channel(id)
    }
}

impl Drop for P2p {
    fn drop(&mut self) {
        self.broadcast_tasks.stop_all_nowait();
    }
}

/// Auxiliary function to broadcast a serialized message concurrently to all given peers.
async fn broadcast_serialized_to<M: Message>(
    message: SerializedMessage,
    channel_list: Vec<ChannelPtr>,
) {
    let futures = FuturesUnordered::new();

    for channel in &channel_list {
        futures.push(
            channel
                .send_serialized(&message, &M::METERING_SCORE, &M::METERING_CONFIGURATION)
                .map_err(|e| {
                    verbose!(
                        target: "net::p2p::broadcast",
                        "[P2P] Broadcasting message to {} failed: {e}",
                        channel.display_address()
                    );
                    // If the channel is stopped then it should automatically die
                    // and the session will remove it from p2p.
                    assert!(channel.is_stopped());
                }),
        );
    }

    let _results: Vec<_> = futures.collect().await;
}
