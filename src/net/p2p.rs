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

use std::sync::Arc;

use futures::{stream::FuturesUnordered, TryFutureExt};
use log::{debug, error, info, warn};
use smol::{lock::Mutex, stream::StreamExt};
use url::Url;

use super::{
    channel::ChannelPtr,
    dnet::DnetEvent,
    hosts::{Hosts, HostsPtr},
    message::Message,
    protocol::{protocol_registry::ProtocolRegistry, register_default_protocols},
    session::{
        InboundSession, InboundSessionPtr, ManualSession, ManualSessionPtr, OutboundSession,
        OutboundSessionPtr, RefineSession, RefineSessionPtr, SeedSyncSession,
    },
    settings::{Settings, SettingsPtr},
};
use crate::{
    system::{ExecutorPtr, Subscriber, SubscriberPtr, Subscription},
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
    settings: SettingsPtr,
    /// Reference to configured [`ManualSession`]
    session_manual: ManualSessionPtr,
    /// Reference to configured [`InboundSession`]
    session_inbound: InboundSessionPtr,
    /// Reference to configured [`OutboundSession`]
    session_outbound: OutboundSessionPtr,
    /// Reference to configured [`RefineSession`]
    session_refine: RefineSessionPtr,

    /// Enable network debugging
    pub dnet_enabled: Mutex<bool>,
    /// The subscriber for which we can give dnet info over
    dnet_subscriber: SubscriberPtr<DnetEvent>,
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
    pub async fn new(settings: Settings, executor: ExecutorPtr) -> P2pPtr {
        let settings = Arc::new(settings);

        let self_ = Arc::new(Self {
            executor,
            hosts: Hosts::new(settings.clone()),
            protocol_registry: ProtocolRegistry::new(),
            settings,
            session_manual: ManualSession::new(),
            session_inbound: InboundSession::new(),
            session_outbound: OutboundSession::new(),
            session_refine: RefineSession::new(),

            dnet_enabled: Mutex::new(false),
            dnet_subscriber: Subscriber::new(),
        });

        self_.session_manual.p2p.init(self_.clone());
        self_.session_inbound.p2p.init(self_.clone());
        self_.session_outbound.p2p.init(self_.clone());
        self_.session_refine.p2p.init(self_.clone());

        register_default_protocols(self_.clone()).await;

        self_
    }

    /// Starts inbound, outbound, and manual sessions.
    pub async fn start(self: Arc<Self>) -> Result<()> {
        debug!(target: "net::p2p::start()", "P2P::start() [BEGIN]");
        info!(target: "net::p2p::start()", "[P2P] Starting P2P subsystem");

        // First attempt any set manual connections
        for peer in &self.settings.peers {
            self.session_manual().connect(peer.clone()).await;
        }

        // Start the inbound session
        if let Err(err) = self.session_inbound().start().await {
            error!(target: "net::p2p::start()", "Failed to start inbound session!: {}", err);
            self.session_manual().stop().await;
            return Err(err)
        }

        // Start the refine session
        self.session_refine().start().await;

        // Start the outbound session
        self.session_outbound().start().await;

        info!(target: "net::p2p::start()", "[P2P] P2P subsystem started");
        Ok(())
    }

    /// Reseed the P2P network.
    pub async fn seed(self: Arc<Self>) -> Result<()> {
        debug!(target: "net::p2p::seed()", "P2P::seed() [BEGIN]");
        info!(target: "net::p2p::seed()", "[P2P] Seeding P2P subsystem");

        // Start seed session
        let seed = SeedSyncSession::new(Arc::downgrade(&self));
        // This will block until all seed queries have finished
        seed.start().await?;

        debug!(target: "net::p2p::seed()", "P2P::seed() [END]");
        Ok(())
    }

    /// Stop the running P2P subsystem
    pub async fn stop(&self) {
        // Stop all channels
        for channel in self.hosts.channels().await {
            channel.stop().await;
        }

        // Stop the sessions
        self.session_manual().stop().await;
        self.session_inbound().stop().await;
        self.session_refine().stop().await;
        self.session_outbound().stop().await;
    }

    /// Broadcasts a message concurrently across all active channels.
    pub async fn broadcast<M: Message>(&self, message: &M) {
        self.broadcast_with_exclude(message, &[]).await
    }

    /// Broadcasts a message concurrently across active channels, excluding
    /// the ones provided in `exclude_list`.
    pub async fn broadcast_with_exclude<M: Message>(&self, message: &M, exclude_list: &[Url]) {
        let mut channels = Vec::new();
        for channel in self.hosts().channels().await {
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

    pub async fn is_connected(&self) -> bool {
        !self.hosts().channels().await.is_empty()
    }

    /// Return an atomic pointer to the set network settings
    pub fn settings(&self) -> SettingsPtr {
        self.settings.clone()
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

    /// Enable network debugging
    pub async fn dnet_enable(&self) {
        *self.dnet_enabled.lock().await = true;
        warn!("[P2P] Network debugging enabled!");
    }

    /// Disable network debugging
    pub async fn dnet_disable(&self) {
        *self.dnet_enabled.lock().await = false;
        warn!("[P2P] Network debugging disabled!");
    }

    /// Subscribe to dnet events
    pub async fn dnet_subscribe(&self) -> Subscription<DnetEvent> {
        self.dnet_subscriber.clone().subscribe().await
    }

    /// Send a dnet notification over the subscriber
    pub(super) async fn dnet_notify(&self, event: DnetEvent) {
        self.dnet_subscriber.notify(event).await;
    }
}
