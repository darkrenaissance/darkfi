/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use futures::{stream::FuturesUnordered, TryFutureExt};
use futures_rustls::rustls::crypto::{ring, CryptoProvider};
use log::{debug, error, info, warn};
use smol::{
    fs::{self, unix::PermissionsExt},
    lock::RwLock as AsyncRwLock,
    stream::StreamExt,
};
use url::Url;

use super::{
    channel::ChannelPtr,
    dnet::DnetEvent,
    hosts::{Hosts, HostsPtr},
    message::Message,
    protocol::{protocol_registry::ProtocolRegistry, register_default_protocols},
    session::{
        InboundSession, InboundSessionPtr, ManualSession, ManualSessionPtr, OutboundSession,
        OutboundSessionPtr, RefineSession, RefineSessionPtr, SeedSyncSession, SeedSyncSessionPtr,
    },
    settings::Settings,
};
use crate::{
    system::{ExecutorPtr, Publisher, PublisherPtr, Subscription},
    util::path::expand_path,
    Result,
};

/// Atomic pointer to the p2p interface
pub type P2pPtr = Arc<P2p>;

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
    /// Enable network debugging
    pub dnet_enabled: AtomicBool,
    /// The publisher for which we can give dnet info over
    dnet_publisher: PublisherPtr<DnetEvent>,
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
        if let Some(ref datastore) = settings.datastore {
            let datastore = expand_path(datastore)?;
            fs::create_dir_all(&datastore).await?;
            fs::set_permissions(&datastore, PermissionsExt::from_mode(0o700)).await?;
        }

        // Register a CryptoProvider for rustls
        let _ = CryptoProvider::install_default(ring::default_provider());

        // Wrap the Settings into an Arc<RwLock>
        let settings = Arc::new(AsyncRwLock::new(settings));

        let self_ = Arc::new(Self {
            executor,
            hosts: Hosts::new(Arc::clone(&settings)),
            protocol_registry: ProtocolRegistry::new(),
            settings,
            session_manual: ManualSession::new(),
            session_inbound: InboundSession::new(),
            session_outbound: OutboundSession::new(),
            session_refine: RefineSession::new(),
            session_seedsync: SeedSyncSession::new(),

            dnet_enabled: AtomicBool::new(false),
            dnet_publisher: Publisher::new(),
        });

        self_.session_inbound.p2p.init(self_.clone());
        self_.session_manual.p2p.init(self_.clone());
        self_.session_seedsync.p2p.init(self_.clone());
        self_.session_outbound.p2p.init(self_.clone());
        self_.session_refine.p2p.init(self_.clone());

        register_default_protocols(self_.clone()).await;

        Ok(self_)
    }

    /// Starts inbound, outbound, and manual sessions.
    pub async fn start(self: Arc<Self>) -> Result<()> {
        debug!(target: "net::p2p::start", "P2P::start() [BEGIN]");
        info!(target: "net::p2p::start", "[P2P] Starting P2P subsystem");

        // Start the inbound session
        if let Err(err) = self.session_inbound().start().await {
            error!(target: "net::p2p::start", "Failed to start inbound session!: {}", err);
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

        info!(target: "net::p2p::start", "[P2P] P2P subsystem started");
        Ok(())
    }

    /// Reseed the P2P network.
    pub async fn seed(self: Arc<Self>) {
        debug!(target: "net::p2p::seed()", "P2P::seed() [BEGIN]");
        info!(target: "net::p2p::seed()", "[P2P] Seeding P2P subsystem");

        // Activate the seed session.
        self.session_seedsync().notify().await;

        debug!(target: "net::p2p::seed()", "P2P::seed() [END]");
    }

    /// Stop the running P2P subsystem
    pub async fn stop(&self) {
        // Stop the sessions
        self.session_manual().stop().await;
        self.session_inbound().stop().await;
        self.session_seedsync().stop().await;
        self.session_outbound().stop().await;
        self.session_refine().stop().await;
    }

    /// Broadcasts a message concurrently across all active channels.
    pub async fn broadcast<M: Message>(&self, message: &M) {
        self.broadcast_with_exclude(message, &[]).await
    }

    /// Broadcasts a message concurrently across active channels, excluding
    /// the ones provided in `exclude_list`.
    pub async fn broadcast_with_exclude<M: Message>(&self, message: &M, exclude_list: &[Url]) {
        let mut channels = Vec::new();
        for channel in self.hosts().channels() {
            if exclude_list.contains(channel.address()) {
                continue
            }
            channels.push(channel);
        }
        self.broadcast_to(message, &channels).await
    }

    /// Broadcast a message concurrently to all given peers.
    pub async fn broadcast_to<M: Message>(&self, message: &M, channel_list: &[ChannelPtr]) {
        if channel_list.is_empty() {
            warn!(target: "net::p2p::broadcast()", "[P2P] No connected channels found for broadcast");
            return
        }

        let futures = FuturesUnordered::new();

        for channel in channel_list {
            futures.push(channel.send(message).map_err(|e| {
                error!(
                    target: "net::p2p::broadcast()",
                    "[P2P] Broadcasting message to {} failed: {}",
                    channel.address(), e
                );
                // If the channel is stopped then it should automatically die
                // and the session will remove it from p2p.
                assert!(channel.is_stopped());
            }));
        }

        let _results: Vec<_> = futures.collect().await;
    }

    pub fn is_connected(&self) -> bool {
        !self.hosts().channels().is_empty()
    }

    /// Return an atomic pointer to the set network settings
    pub fn settings(&self) -> Arc<AsyncRwLock<Settings>> {
        Arc::clone(&self.settings)
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

    /// Enable network debugging
    pub fn dnet_enable(&self) {
        self.dnet_enabled.store(true, Ordering::SeqCst);
        warn!("[P2P] Network debugging enabled!");
    }

    /// Disable network debugging
    pub fn dnet_disable(&self) {
        self.dnet_enabled.store(false, Ordering::SeqCst);
        warn!("[P2P] Network debugging disabled!");
    }

    /// Subscribe to dnet events
    pub async fn dnet_subscribe(&self) -> Subscription<DnetEvent> {
        self.dnet_publisher.clone().subscribe().await
    }

    /// Send a dnet notification over the publisher
    pub(super) async fn dnet_notify(&self, event: DnetEvent) {
        self.dnet_publisher.notify(event).await;
    }
}
