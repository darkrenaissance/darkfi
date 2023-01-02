/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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
    fmt,
};

use async_std::sync::{Arc, Mutex};
use futures::{select, stream::FuturesUnordered, try_join, FutureExt, StreamExt, TryFutureExt};
use log::{debug, error, warn};
use rand::Rng;
use serde_json::json;
use smol::Executor;
use url::Url;

use crate::{
    system::{Subscriber, SubscriberPtr, Subscription},
    util::async_util::sleep,
    Result,
};

use super::{
    message::Message,
    protocol::{register_default_protocols, ProtocolRegistry},
    session::{InboundSession, ManualSession, OutboundSession, SeedSyncSession, Session},
    Channel, ChannelPtr, Hosts, HostsPtr, Settings, SettingsPtr,
};

/// List of channels that are awaiting connection.
pub type PendingChannels = Mutex<HashSet<Url>>;
/// List of connected channels.
pub type ConnectedChannels = Mutex<HashMap<Url, Arc<Channel>>>;
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
    stop_subscriber: SubscriberPtr<()>,
    hosts: HostsPtr,
    protocol_registry: ProtocolRegistry,

    // We keep a reference to the sessions used for get info
    session_manual: Mutex<Option<Arc<ManualSession>>>,
    session_inbound: Mutex<Option<Arc<InboundSession>>>,
    session_outbound: Mutex<Option<Arc<OutboundSession>>>,

    state: Mutex<P2pState>,

    settings: SettingsPtr,

    /// Flag to check if on discovery mode
    discovery: Mutex<bool>,
}

impl P2p {
    /// Initialize a new p2p network.
    ///
    /// Initializes all sessions and protocols. Adds the protocols to the protocol registry, along
    /// with a bitflag session selector that includes or excludes sessions from seed, version, and
    /// address protocols.
    ///
    /// Creates a weak pointer to self that is used by all sessions to access the p2p parent class.
    pub async fn new(settings: Settings) -> Arc<Self> {
        let settings = Arc::new(settings);

        let self_ = Arc::new(Self {
            pending: Mutex::new(HashSet::new()),
            channels: Mutex::new(HashMap::new()),
            channel_subscriber: Subscriber::new(),
            stop_subscriber: Subscriber::new(),
            hosts: Hosts::new(settings.localnet),
            protocol_registry: ProtocolRegistry::new(),
            session_manual: Mutex::new(None),
            session_inbound: Mutex::new(None),
            session_outbound: Mutex::new(None),
            state: Mutex::new(P2pState::Open),
            settings,
            discovery: Mutex::new(false),
        });

        let parent = Arc::downgrade(&self_);

        *self_.session_manual.lock().await = Some(ManualSession::new(parent.clone()));
        *self_.session_inbound.lock().await = Some(InboundSession::new(parent.clone()).await);
        *self_.session_outbound.lock().await = Some(OutboundSession::new(parent));

        register_default_protocols(self_.clone()).await;

        self_
    }

    // ANCHOR: get_info
    pub async fn get_info(&self) -> serde_json::Value {
        // Building ext_addr_vec string
        let mut ext_addr_vec = vec![];
        for ext_addr in &self.settings.external_addr {
            ext_addr_vec.push(ext_addr.as_ref().to_string());
        }

        json!({
            "external_addr": format!("{:?}", ext_addr_vec),
            "session_manual": self.session_manual().await.get_info().await,
            "session_inbound": self.session_inbound().await.get_info().await,
            "session_outbound": self.session_outbound().await.get_info().await,
            "state": self.state.lock().await.to_string(),
        })
    }
    // ANCHOR_END: get_info

    /// Invoke startup and seeding sequence. Call from constructing thread.
    // ANCHOR: start
    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::p2p::start()", "P2p::start() [BEGIN]");

        *self.state.lock().await = P2pState::Start;

        // Start seed session
        let seed = SeedSyncSession::new(Arc::downgrade(&self));
        // This will block until all seed queries have finished
        seed.start(executor.clone()).await?;

        *self.state.lock().await = P2pState::Started;

        debug!(target: "net::p2p::start()", "P2p::start() [END]");
        Ok(())
    }
    // ANCHOR_END: start

    pub async fn session_manual(&self) -> Arc<ManualSession> {
        self.session_manual.lock().await.as_ref().unwrap().clone()
    }
    pub async fn session_inbound(&self) -> Arc<InboundSession> {
        self.session_inbound.lock().await.as_ref().unwrap().clone()
    }
    pub async fn session_outbound(&self) -> Arc<OutboundSession> {
        self.session_outbound.lock().await.as_ref().unwrap().clone()
    }

    /// Runs the network. Starts inbound, outbound and manual sessions.
    /// Waits for a stop signal and stops the network if received.
    // ANCHOR: run
    pub async fn run(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::p2p::run()", "P2p::run() [BEGIN]");

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

        debug!(target: "net::p2p::run()", "P2p::run() [END]");
        Ok(())
    }
    // ANCHOR_END: run

    /// Wait for outbound connections to be established.
    pub async fn wait_for_outbound(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::p2p::wait_for_outbound()", "P2p::wait_for_outbound() [BEGIN]");
        // To verify that the network needs initialization, we check if we have seeds or peers configured,
        // and have configured outbound slots.
        if !(self.settings.seeds.is_empty() && self.settings.peers.is_empty()) &&
            self.settings.outbound_connections > 0
        {
            debug!(target: "net::p2p::wait_for_outbound()", "P2p::wait_for_outbound(): seeds are configured, waiting for outbound initialization...");
            // Retrieve P2P network settings;
            let settings = self.settings();

            // Retrieve our own inbound addresses
            let self_inbound_addr = &settings.external_addr;

            // Retrieve timeout config
            let timeout = settings.connect_timeout_seconds as u64;

            // Retrieve outbound addresses to connect to (including manual peers)
            let peers = &settings.peers;
            let outbound = &self.hosts().load_all().await;

            // Enable manual channel subscriber notifications
            self.session_manual().await.clone().enable_notify().await;

            // Retrieve manual channel subscriber ptr
            let manual_sub =
                self.session_manual.lock().await.as_ref().unwrap().subscribe_channel().await;

            // Enable outbound channel subscriber notifications
            self.session_outbound().await.clone().enable_notify().await;

            // Retrieve outbound channel subscriber ptr
            let outbound_sub =
                self.session_outbound.lock().await.as_ref().unwrap().subscribe_channel().await;

            // Create tasks for peers and outbound
            let peers_task = Self::outbound_addr_loop(
                self_inbound_addr,
                timeout,
                self.subscribe_stop().await,
                peers,
                manual_sub,
                executor.clone(),
            );
            let outbound_task = Self::outbound_addr_loop(
                self_inbound_addr,
                timeout,
                self.subscribe_stop().await,
                outbound,
                outbound_sub,
                executor,
            );
            // Wait for both tasks completion
            try_join!(peers_task, outbound_task)?;

            // Disable manual channel subscriber notifications
            self.session_manual().await.disable_notify().await;

            // Disable outbound channel subscriber notifications
            self.session_outbound().await.disable_notify().await;
        }

        debug!(target: "net::p2p::wait_for_outbound()", "P2p::wait_for_outbound() [END]");
        Ok(())
    }

    // Wait for the process for each of the provided addresses, excluding our own inbound addresses
    async fn outbound_addr_loop(
        self_inbound_addr: &[Url],
        timeout: u64,
        stop_sub: Subscription<()>,
        addrs: &Vec<Url>,
        subscriber: Subscription<Result<ChannelPtr>>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        // Process addresses
        for addr in addrs {
            if self_inbound_addr.contains(addr) {
                continue
            }

            // Wait for address to be processed.
            // We use a timeout to eliminate the following cases:
            //  1. Network timeout
            //  2. Thread reaching the receiver after peer has signal it
            let (timeout_s, timeout_r) = smol::channel::unbounded::<()>();
            executor
                .spawn(async move {
                    sleep(timeout).await;
                    timeout_s.send(()).await.unwrap_or(());
                })
                .detach();

            select! {
                msg = subscriber.receive().fuse() => {
                        if let Err(e) = msg {
                            warn!(
                                target: "net::p2p::outbound_addr_loop()",
                                "P2p::wait_for_outbound(): Outbound connection failed [{}]: {}",
                                addr, e
                            );
                        }
                },
                _ = stop_sub.receive().fuse() => debug!(target: "net::p2p::outbound_addr_loop()", "P2p::wait_for_outbound(): stop signal received!"),
                _ = timeout_r.recv().fuse() => {
                    warn!(target: "net::p2p::outbound_addr_loop()", "P2p::wait_for_outbound(): Timeout on outbound connection: {}", addr);
                    continue
                },
            }
        }

        Ok(())
    }

    // ANCHOR: stop
    pub async fn stop(&self) {
        self.stop_subscriber.notify(()).await
    }
    // ANCHOR_END: stop

    /// Broadcasts a message concurrently across all channels.
    // ANCHOR: broadcast
    pub async fn broadcast<M: Message + Clone>(&self, message: M) -> Result<()> {
        let chans = self.channels.lock().await;
        let iter = chans.values();
        let mut futures = FuturesUnordered::new();

        for channel in iter {
            futures.push(channel.send(message.clone()).map_err(|e| {
                format!(
                    "P2P::broadcast: Broadcasting message to {} failed: {}",
                    channel.address(),
                    e
                )
            }));
        }

        if futures.is_empty() {
            error!(target: "net::p2p::broadcast()", "P2P::broadcast: No connected channels found");
            return Ok(())
        }

        while let Some(entry) = futures.next().await {
            if let Err(e) = entry {
                error!(target: "net::p2p::broadcast()", "{}", e);
            }
        }

        Ok(())
    }
    // ANCHOR_END: broadcast

    /// Broadcasts a message concurrently across all channels.
    /// Excludes channels provided in `exclude_list`.
    pub async fn broadcast_with_exclude<M: Message + Clone>(
        &self,
        message: M,
        exclude_list: &[Url],
    ) -> Result<()> {
        let chans = self.channels.lock().await;
        let iter = chans.values();
        let mut futures = FuturesUnordered::new();

        for channel in iter {
            if !exclude_list.contains(&channel.address()) {
                futures.push(channel.send(message.clone()).map_err(|e| {
                    format!(
                        "P2P::broadcast_with_exclude: Broadcasting message to {} failed: {}",
                        channel.address(),
                        e
                    )
                }));
            }
        }

        if futures.is_empty() {
            error!(target: "net::p2p::broadcast_with_exclude()", "P2P::broadcast_with_exclude: No connected channels found");
            return Ok(())
        }

        while let Some(entry) = futures.next().await {
            if let Err(e) = entry {
                error!(target: "net::p2p::broadcast_with_exclude()", "{}", e);
            }
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
    /// If key is not contained, we also check if we are connected with a different transport.
    pub async fn exists(&self, addr: &Url) -> Result<bool> {
        let channels = self.channels.lock().await;
        if channels.contains_key(addr) {
            return Ok(true)
        }

        let mut addr = addr.clone();
        for transport in &self.settings.outbound_transports {
            addr.set_scheme(&transport.to_scheme())?;
            if channels.contains_key(&addr) {
                return Ok(true)
            }
        }

        Ok(false)
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
    pub async fn subscribe_stop(&self) -> Subscription<()> {
        self.stop_subscriber.clone().subscribe().await
    }

    /// Retrieve channels
    pub fn channels(&self) -> &ConnectedChannels {
        &self.channels
    }

    /// Try to start discovery mode.
    /// Returns false if already on discovery mode.
    pub async fn start_discovery(self: Arc<Self>) -> bool {
        if *self.discovery.lock().await {
            return false
        }
        *self.discovery.lock().await = true;
        true
    }

    /// Stops discovery mode.
    pub async fn stop_discovery(self: Arc<Self>) {
        *self.discovery.lock().await = false;
    }

    /// Retrieves a random connected channel, exluding seeds
    pub async fn random_channel(self: Arc<Self>) -> Option<Arc<Channel>> {
        let mut channels_map = self.channels().lock().await.clone();
        channels_map.retain(|c, _| !self.settings.seeds.contains(c));
        let mut values = channels_map.values();

        if values.len() == 0 {
            return None
        }

        Some(values.nth(rand::thread_rng().gen_range(0..values.len())).unwrap().clone())
    }
}
