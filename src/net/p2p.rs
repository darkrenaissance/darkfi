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

use std::collections::{HashMap, HashSet};

use async_std::{
    stream::StreamExt,
    sync::{Arc, Mutex},
};
use futures::{stream::FuturesUnordered, TryFutureExt};
use log::{debug, error, info, warn};
use rand::{prelude::IteratorRandom, rngs::OsRng};
use smol::Executor;
use url::Url;

use super::{
    channel::ChannelPtr,
    hosts::{Hosts, HostsPtr},
    message::Message,
    protocol::{protocol_registry::ProtocolRegistry, register_default_protocols},
    session::{
        inbound_session::InboundDnet,
        outbound_session::{OutboundDnet, OutboundState},
        InboundSession, InboundSessionPtr, ManualSession, ManualSessionPtr, OutboundSession,
        OutboundSessionPtr, SeedSyncSession, Session,
    },
    settings::{Settings, SettingsPtr},
};
use crate::{
    system::{Subscriber, SubscriberPtr, Subscription},
    Result,
};

/// Set of channels that are awaiting connection
pub type PendingChannels = Mutex<HashSet<Url>>;
/// Set of connected channels
pub type ConnectedChannels = Mutex<HashMap<Url, ChannelPtr>>;
/// Atomic pointer to the p2p interface
pub type P2pPtr = Arc<P2p>;

/// Representations of the p2p state
enum P2pState {
    /// The P2P object has been created but not yet started
    Open,
    /// We are performing the initial seed session
    Start,
    /// Seed session finished, but not yet running
    Started,
    /// P2P is running and the network is active
    Run,
    /// The P2P network has been stopped
    Stopped,
}

/// Types of DnetInfo (used with sessions)
pub enum DnetInfo {
    /// Hosts info
    Hosts(Vec<Url>),
    /// Outbound Session Info
    Outbound(OutboundDnet),
    /// Inbound Session Info
    Inbound(InboundDnet),
    // Manual Session Info
    //Manual(ManualDnet),
}

impl std::fmt::Display for P2pState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Open => "open",
                Self::Start => "start",
                Self::Started => "started",
                Self::Run => "run",
                Self::Stopped => "stopped",
            }
        )
    }
}

/// Toplevel peer-to-peer networking interface
pub struct P2p {
    /// Channels pending connection
    pending: PendingChannels,
    /// Connected channels
    channels: ConnectedChannels,
    /// Subscriber for notifications of new channels
    channel_subscriber: SubscriberPtr<Result<ChannelPtr>>,
    /// Subscriber for stop notifications
    stop_subscriber: SubscriberPtr<()>,
    /// Known hosts (peers)
    hosts: HostsPtr,
    /// Protocol registry
    protocol_registry: ProtocolRegistry,
    /// The state of the interface
    state: Mutex<P2pState>,
    /// P2P network settings
    settings: SettingsPtr,
    /// Boolean lock marking if peer discovery is active
    pub peer_discovery_running: Mutex<bool>,

    /// Reference to configured [`ManualSession`]
    session_manual: Mutex<Option<Arc<ManualSession>>>,
    /// Reference to configured [`InboundSession`]
    session_inbound: Mutex<Option<Arc<InboundSession>>>,
    /// Reference to configured [`OutboundSession`]
    session_outbound: Mutex<Option<Arc<OutboundSession>>>,

    /// Enable network debugging
    pub dnet_enabled: Mutex<bool>,
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
    pub async fn new(settings: Settings) -> P2pPtr {
        let settings = Arc::new(settings);

        let self_ = Arc::new(Self {
            pending: Mutex::new(HashSet::new()),
            channels: Mutex::new(HashMap::new()),
            channel_subscriber: Subscriber::new(),
            stop_subscriber: Subscriber::new(),
            hosts: Hosts::new(settings.clone()),
            protocol_registry: ProtocolRegistry::new(),
            state: Mutex::new(P2pState::Open),
            settings,
            peer_discovery_running: Mutex::new(false),

            session_manual: Mutex::new(None),
            session_inbound: Mutex::new(None),
            session_outbound: Mutex::new(None),

            dnet_enabled: Mutex::new(false),
        });

        let parent = Arc::downgrade(&self_);

        *self_.session_manual.lock().await = Some(ManualSession::new(parent.clone()));
        *self_.session_inbound.lock().await = Some(InboundSession::new(parent.clone()));
        *self_.session_outbound.lock().await = Some(OutboundSession::new(parent));

        register_default_protocols(self_.clone()).await;

        self_
    }

    /// Invoke startup and seeding sequence. Call from constructing thread.
    pub async fn start(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::p2p::start()", "P2P::start() [BEGIN]");
        info!(target: "net::p2p::start()", "[P2P] Seeding P2P subsystem");
        *self.state.lock().await = P2pState::Start;

        // Start seed session
        let seed = SeedSyncSession::new(Arc::downgrade(&self));
        // This will block until all seed queries have finished
        seed.start(ex.clone()).await?;

        *self.state.lock().await = P2pState::Started;

        debug!(target: "net::p2p::start()", "P2P::start() [END]");
        Ok(())
    }

    /// Reseed the P2P network.
    pub async fn reseed(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::p2p::reseed()", "P2P::reseed() [BEGIN]");
        info!(target: "net::p2p::reseed()", "[P2P] Reseeding P2P subsystem");

        // Start seed session
        let seed = SeedSyncSession::new(Arc::downgrade(&self));
        // This will block until all seed queries have finished
        seed.start(ex.clone()).await?;

        debug!(target: "net::p2p::reseed()", "P2P::reseed() [END]");
        Ok(())
    }

    /// Runs the network. Starts inbound, outbound, and manual sessions.
    /// Waits for a stop signal and stops the network if received.
    pub async fn run(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::p2p::run()", "P2P::run() [BEGIN]");
        info!(target: "net::p2p::run()", "[P2P] Running P2P subsystem");
        *self.state.lock().await = P2pState::Run;

        // First attempt any set manual connections
        let manual = self.session_manual().await;
        for peer in &self.settings.peers {
            manual.clone().connect(peer.clone(), ex.clone()).await;
        }

        // Start the inbound session
        let inbound = self.session_inbound().await;
        inbound.clone().start(ex.clone()).await?;

        // Start the outbound session
        let outbound = self.session_outbound().await;
        outbound.clone().start(ex.clone()).await?;

        info!(target: "net::p2p::run()", "[P2P] P2P subsystem started");

        // Wait for stop signal
        let stop_sub = self.subscribe_stop().await;
        stop_sub.receive().await;

        info!(target: "net::p2p::run()", "[P2P] Received P2P subsystem stop signal. Shutting down.");

        // Stop the sessions
        manual.stop().await;
        inbound.stop().await;
        outbound.stop().await;

        *self.state.lock().await = P2pState::Stopped;

        debug!(target: "net::p2p::run()", "P2P::run() [END]");
        Ok(())
    }

    /// Subscribe to a stop signal.
    pub async fn subscribe_stop(&self) -> Subscription<()> {
        self.stop_subscriber.clone().subscribe().await
    }

    /// Stop the running P2P subsystem
    pub async fn stop(&self) {
        self.stop_subscriber.notify(()).await
    }

    /// Add a channel to the set of connected channels
    pub async fn store(&self, channel: ChannelPtr) {
        // TODO: Check the code path for this, and potentially also insert the remote
        // into the hosts list?
        self.channels.lock().await.insert(channel.address().clone(), channel.clone());
        self.channel_subscriber.notify(Ok(channel)).await;
    }

    /// Broadcasts a message concurrently across all active channels.
    pub async fn broadcast<M: Message>(&self, message: &M) {
        self.broadcast_with_exclude(message, &[]).await
    }

    /// Broadcasts a message concurrently across active channels, excluding
    /// the ones provided in `exclude_list`.
    pub async fn broadcast_with_exclude<M: Message>(&self, message: &M, exclude_list: &[Url]) {
        let chans = self.channels.lock().await;
        let iter = chans.values();
        let mut futures = FuturesUnordered::new();

        for channel in iter {
            if exclude_list.contains(channel.address()) {
                continue
            }

            futures.push(channel.send(message).map_err(|e| {
                (
                    format!("[P2P] Broadcasting message to {} failed: {}", channel.address(), e),
                    channel.clone(),
                )
            }));
        }

        if futures.is_empty() {
            warn!(target: "net::p2p::broadcast()", "[P2P] No connected channels found for broadcast");
            return
        }

        while let Some(entry) = futures.next().await {
            if let Err((e, chan)) = entry {
                error!(target: "net::p2p::broadcast()", "{}", e);
                self.remove(chan).await;
            }
        }
    }

    /// Check whether we're connected to a given address
    pub async fn exists(&self, addr: &Url) -> bool {
        self.channels.lock().await.contains_key(addr)
    }

    /// Remove a channel from the set of connected channels
    pub async fn remove(&self, channel: ChannelPtr) {
        self.channels.lock().await.remove(channel.address());
    }

    /// Add an address to the list of pending channels.
    pub async fn add_pending(&self, addr: &Url) -> bool {
        self.pending.lock().await.insert(addr.clone())
    }

    /// Remove a channel from the list of pending channels.
    pub async fn remove_pending(&self, addr: &Url) {
        self.pending.lock().await.remove(addr);
    }

    /// Return reference to connected channels map
    pub fn channels(&self) -> &ConnectedChannels {
        &self.channels
    }

    /// Retrieve a random connected channel from the
    pub async fn random_channel(&self) -> Option<ChannelPtr> {
        let channels = self.channels().lock().await;
        channels.values().choose(&mut OsRng).cloned()
    }

    /// Return an atomic pointer to the set network settings
    pub fn settings(&self) -> SettingsPtr {
        self.settings.clone()
    }

    /// Return an atomic pointer to the list of hosts
    pub fn hosts(&self) -> HostsPtr {
        self.hosts.clone()
    }

    /// Return a reference to the internal protocol registry
    pub fn protocol_registry(&self) -> &ProtocolRegistry {
        &self.protocol_registry
    }

    /// Get pointer to manual session
    pub async fn session_manual(&self) -> ManualSessionPtr {
        self.session_manual.lock().await.as_ref().unwrap().clone()
    }

    /// Get pointer to inbound session
    pub async fn session_inbound(&self) -> InboundSessionPtr {
        self.session_inbound.lock().await.as_ref().unwrap().clone()
    }

    /// Get pointer to outbound session
    pub async fn session_outbound(&self) -> OutboundSessionPtr {
        self.session_outbound.lock().await.as_ref().unwrap().clone()
    }

    /// Enable network debugging
    pub async fn dnet_enable(&self) {
        // Enable log for all connected channels if not enabled already
        for channel in self.channels().lock().await.values() {
            channel.dnet_enable().await;
        }

        *self.dnet_enabled.lock().await = true;
        warn!("[P2P] Network debugging enabled!");
    }

    /// Disable network debugging
    pub async fn dnet_disable(&self) {
        *self.dnet_enabled.lock().await = false;

        // Clear out any held data
        for channel in self.channels().lock().await.values() {
            channel.dnet_disable().await;
        }

        warn!("[P2P] Network debugging disabled!");
    }

    /// Gather session dnet info and return it in a vec.
    /// Returns an empty vec if dnet is disabled.
    pub async fn dnet_info(&self) -> Vec<DnetInfo> {
        let mut ret = vec![];

        if *self.dnet_enabled.lock().await {
            ret.push(self.session_inbound().await.dnet_info().await);
            ret.push(self.session_outbound().await.dnet_info().await);
            ret.push(DnetInfo::Hosts(self.hosts.load_all().await));
        }

        ret
    }

    /// Maps DnetInfo into a JSON struct usable by clients
    pub fn map_dnet_info(dnet_info: Vec<DnetInfo>) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert("inbound".into(), serde_json::Value::Null);
        map.insert("outbound".into(), serde_json::Value::Null);
        map.insert("hosts".into(), serde_json::Value::Null);

        // We assume there will be one of each
        for info in dnet_info {
            match info {
                DnetInfo::Hosts(hosts) => map["hosts"] = serde_json::json!(hosts),

                DnetInfo::Outbound(outbound_info) => {
                    let mut slot_info = vec![];
                    for slot in outbound_info.slots {
                        let Some(slot) = slot else {
                            slot_info.push(serde_json::Value::Null);
                            continue
                        };

                        let obj = if slot.state != OutboundState::Open {
                            serde_json::json!({
                                "addr": slot.addr.unwrap().to_string(),
                                "state": slot.state.to_string(),
                                "info": {
                                    "addr": slot.channel.as_ref().unwrap().addr.to_string(),
                                    "random_id": slot.channel.as_ref().unwrap().random_id,
                                    "remote_id": slot.channel.as_ref().unwrap().remote_node_id,
                                    "log": slot.channel.as_ref().unwrap().log.to_vec(),
                                }
                            })
                        } else {
                            serde_json::json!({
                                "addr": serde_json::Value::Null,
                                "state": slot.state.to_string(),
                                "info": serde_json::Value::Null,
                            })
                        };

                        slot_info.push(obj);
                    }

                    map["outbound"] = serde_json::json!(slot_info);
                }

                DnetInfo::Inbound(inbound_info) => {
                    let mut slot_info = vec![];
                    for slot in inbound_info.slots {
                        let Some(slot) = slot else {
                            slot_info.push(serde_json::Value::Null);
                            continue
                        };

                        let obj = serde_json::json!({
                            "addr": slot.addr.unwrap().to_string(),
                            "info": {
                                "addr": slot.channel.as_ref().unwrap().addr.to_string(),
                                "random_id": slot.channel.as_ref().unwrap().random_id,
                                "remote_id": slot.channel.as_ref().unwrap().remote_node_id,
                                "log": slot.channel.as_ref().unwrap().log.to_vec(),
                            }
                        });

                        slot_info.push(obj);
                    }

                    map["inbound"] = serde_json::json!(slot_info);
                }
            }
        }

        serde_json::json!(map)
    }
}

macro_rules! dnet {
    ($self:expr, $($code:tt)*) => {
        {
            if *$self.p2p().dnet_enabled.lock().await {
                $($code)*
            }
        }
    };
}
pub(crate) use dnet;
