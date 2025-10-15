/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

//! Outbound connections session. Manages the creation of outbound sessions.
//! Used to create an outbound session and to stop and start the session.
//!
//! Class consists of a weak pointer to the p2p interface and a vector of
//! outbound connection slots. Using a weak pointer to p2p allows us to
//! avoid circular dependencies. The vector of slots is wrapped in a mutex
//! lock. This is switched on every time we instantiate a connection slot
//! and insures that no other part of the program uses the slots at the
//! same time.

use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Weak,
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use log::{debug, error, info, warn};
use smol::lock::Mutex;
use url::Url;

use super::{
    super::{
        channel::ChannelPtr,
        connector::Connector,
        dnet::{self, dnetev, DnetEvent},
        hosts::{HostColor, HostState},
        message::GetAddrsMessage,
        p2p::{P2p, P2pPtr},
    },
    Session, SessionBitFlag, SESSION_OUTBOUND,
};
use crate::{
    system::{sleep, timeout::timeout, CondVar, StoppableTask, StoppableTaskPtr},
    Error, Result,
};

pub type OutboundSessionPtr = Arc<OutboundSession>;

/// Defines outbound connections session.
pub struct OutboundSession {
    /// Weak pointer to parent p2p object
    pub(in crate::net) p2p: Weak<P2p>,
    /// Outbound connection slots
    slots: Mutex<Vec<Arc<Slot>>>,
    /// Peer discovery task
    peer_discovery: Arc<PeerDiscovery>,
}

impl OutboundSession {
    /// Create a new outbound session.
    pub(crate) fn new(p2p: Weak<P2p>) -> OutboundSessionPtr {
        Arc::new_cyclic(|session| Self {
            p2p,
            slots: Mutex::new(Vec::new()),
            peer_discovery: PeerDiscovery::new(session.clone()),
        })
    }

    /// Start the outbound session. Runs the channel connect loop.
    pub(crate) async fn start(self: Arc<Self>) {
        let n_slots = self.p2p().settings().read().await.outbound_connections;
        info!(target: "net::outbound_session", "[P2P] Starting {n_slots} outbound connection slots.");

        // Activate mutex lock on connection slots.
        let mut slots = self.slots.lock().await;

        let mut futures = FuturesUnordered::new();

        let self_ = Arc::downgrade(&self);

        for i in 0..n_slots as u32 {
            let slot = Slot::new(self_.clone(), i);
            futures.push(slot.clone().start());
            slots.push(slot);
        }

        while (futures.next().await).is_some() {}

        self.peer_discovery.clone().start().await;
    }

    /// Stops the outbound session.
    pub(crate) async fn stop(&self) {
        debug!(target: "net::outbound_session", "Stopping outbound session..");
        let slots = &*self.slots.lock().await;
        let mut futures = FuturesUnordered::new();

        for slot in slots {
            futures.push(slot.clone().stop());
        }

        while (futures.next().await).is_some() {}

        self.peer_discovery.clone().stop().await;
        debug!(target: "net::outbound_session", "Outbound session stopped!");
    }

    pub async fn slot_info(&self) -> Vec<u32> {
        let mut info = Vec::new();
        let slots = &*self.slots.lock().await;
        for slot in slots {
            info.push(slot.channel_id.load(Ordering::Relaxed));
        }
        info
    }

    fn wakeup_peer_discovery(&self) {
        self.peer_discovery.notify()
    }

    async fn wakeup_slots(&self) {
        let slots = &*self.slots.lock().await;
        for slot in slots {
            slot.notify();
        }
    }
}

#[async_trait]
impl Session for OutboundSession {
    fn p2p(&self) -> P2pPtr {
        self.p2p.upgrade().unwrap()
    }

    fn type_id(&self) -> SessionBitFlag {
        SESSION_OUTBOUND
    }
}

struct Slot {
    slot: u32,
    process: StoppableTaskPtr,
    wakeup_self: CondVar,
    session: Weak<OutboundSession>,
    connector: Connector,
    // For debugging
    channel_id: AtomicU32,
}

impl Slot {
    fn new(session: Weak<OutboundSession>, slot: u32) -> Arc<Self> {
        let settings = session.upgrade().unwrap().p2p().settings();

        Arc::new(Self {
            slot,
            process: StoppableTask::new(),
            wakeup_self: CondVar::new(),
            session: session.clone(),
            connector: Connector::new(settings, session),
            channel_id: AtomicU32::new(0),
        })
    }

    async fn start(self: Arc<Self>) {
        let ex = self.p2p().executor();

        self.process.clone().start(
            self.run(),
            |res| async {
                match res {
                    Ok(()) | Err(Error::NetworkServiceStopped) => {}
                    Err(e) => error!("net::outbound_session {e}"),
                }
            },
            Error::NetworkServiceStopped,
            ex,
        );
    }

    async fn stop(self: Arc<Self>) {
        self.connector.stop();
        self.process.stop().await;
    }

    /// Address selection algorithm that works as follows: up to
    /// gold_count, select from the goldlist. Up to white_count,
    /// select from the whitelist. For all other slots, select from
    /// the greylist. If none of these preferences are satisfied, do
    /// peer discovery.
    ///
    /// Selecting from the greylist for some % of the slots is necessary
    /// and healthy since we require the network retains some unreliable
    /// connections. A network that purely favors uptime over unreliable
    /// connections may be vulnerable to sybil by attackers with good uptime.
    async fn fetch_addrs(&self) -> Option<(Url, u64)> {
        let hosts = self.p2p().hosts();
        let slot = self.slot as usize;
        let container = &self.p2p().hosts().container;

        // Acquire Settings read lock
        let settings = self.p2p().settings().read_arc().await;

        let white_count = (settings.white_connect_percent * settings.outbound_connections) / 100;
        let gold_count = settings.gold_connect_count;

        let transports = settings.allowed_transports.clone();
        let preference_strict = settings.slot_preference_strict;

        // Drop Settings read lock
        drop(settings);

        let grey_only = hosts.container.is_empty(HostColor::White) &&
            hosts.container.is_empty(HostColor::Gold) &&
            !hosts.container.is_empty(HostColor::Grey);

        // If we only have grey entries, select from the greylist. Otherwise,
        // use the preference defined in settings.
        let addrs = if grey_only && !preference_strict {
            container.fetch_with_schemes(HostColor::Grey as usize, &transports, None)
        } else if slot < gold_count {
            container.fetch_with_schemes(HostColor::Gold as usize, &transports, None)
        } else if slot < white_count {
            container.fetch_with_schemes(HostColor::White as usize, &transports, None)
        } else {
            container.fetch_with_schemes(HostColor::Grey as usize, &transports, None)
        };

        hosts.check_addrs(addrs).await
    }

    // We first try to make connections to the addresses on our gold list. We then find some
    // whitelist connections according to the whitelist percent default. Finally, any remaining
    // connections we make from the greylist.
    async fn run(self: Arc<Self>) -> Result<()> {
        let hosts = self.p2p().hosts();

        loop {
            // Activate the slot
            debug!(
                target: "net::outbound_session::try_connect()",
                "[P2P] Finding a host to connect to for outbound slot #{}",
                self.slot,
            );

            // Do peer discovery if we don't have any peers on the Grey, White or Gold list
            // (first time connecting to the network).
            if hosts.container.is_empty(HostColor::Grey) &&
                hosts.container.is_empty(HostColor::White) &&
                hosts.container.is_empty(HostColor::Gold)
            {
                dnetev!(self, OutboundSlotSleeping, {
                    slot: self.slot,
                });

                self.wakeup_self.reset();
                // Peer discovery
                self.session().wakeup_peer_discovery();
                // Wait to be woken up by peer discovery
                self.wakeup_self.wait().await;

                continue
            }

            let addr = if let Some(addr) = self.fetch_addrs().await {
                debug!(target: "net::outbound_session::run()", "Fetched addr={}, slot #{}", addr.0,
                self.slot);
                addr
            } else {
                debug!(target: "net::outbound_session::run()", "No address found! Activating peer discovery...");
                dnetev!(self, OutboundSlotSleeping, {
                    slot: self.slot,
                });

                self.wakeup_self.reset();
                // Peer discovery
                self.session().wakeup_peer_discovery();
                // Wait to be woken up by peer discovery
                self.wakeup_self.wait().await;

                continue
            };

            let host = addr.0;
            let last_seen = addr.1;
            let slot = self.slot;

            info!(
                target: "net::outbound_session::try_connect()",
                "[P2P] Connecting outbound slot #{slot} [{host}]"
            );

            dnetev!(self, OutboundSlotConnecting, {
                slot,
                addr: host.clone(),
            });

            let (_, channel) = match self.try_connect(host.clone(), last_seen).await {
                Ok(connect_info) => connect_info,
                Err(err) => {
                    debug!(
                        target: "net::outbound_session::try_connect()",
                        "[P2P] Outbound slot #{slot} connection failed: {err}"
                    );

                    dnetev!(self, OutboundSlotDisconnected, {
                        slot,
                        err: err.to_string()
                    });

                    self.channel_id.store(0, Ordering::Relaxed);

                    continue
                }
            };

            // At this point we've managed to connect.
            let stop_sub = channel.subscribe_stop().await?;

            info!(
                target: "net::outbound_session::try_connect()",
                "[P2P] Outbound slot #{slot} connected [{}]",
                channel.display_address()
            );

            dnetev!(self, OutboundSlotConnected, {
                slot: self.slot,
                addr: channel.display_address().clone(),
                channel_id: channel.info.id
            });

            // Setup new channel
            if let Err(err) =
                self.session().register_channel(channel.clone(), self.p2p().executor()).await
            {
                info!(
                    target: "net::outbound_session",
                    "[P2P] Outbound slot #{slot} disconnected: {err}"
                );

                dnetev!(self, OutboundSlotDisconnected, {
                    slot: self.slot,
                    err: err.to_string()
                });

                self.channel_id.store(0, Ordering::Relaxed);

                warn!(
                    target: "net::outbound_session::try_connect()",
                    "[P2P] Suspending addr=[{}] slot #{slot}",
                    channel.display_address()
                );

                // Peer disconnected during the registry process. We'll downgrade this peer now.
                self.p2p().hosts().move_host(channel.address(), last_seen, HostColor::Grey).await?;

                // Mark its state as Suspend, which sends this node to the Refinery for processing.
                self.p2p()
                    .hosts()
                    .try_register(channel.address().clone(), HostState::Suspend)
                    .unwrap();

                continue
            }

            self.channel_id.store(channel.info.id, Ordering::Relaxed);

            // Wait for channel to close
            stop_sub.receive().await;

            self.channel_id.store(0, Ordering::Relaxed);
        }
    }

    /// Start making an outbound connection, using provided [`Connector`].
    /// Tries to find a valid address to connect to, otherwise does peer
    /// discovery. The peer discovery loops until some peer we can connect
    /// to is found. Once connected, registers the channel, removes it from
    /// the list of pending channels, and starts sending messages across the
    /// channel. In case of any failures, a network error is returned and the
    /// main connect loop (parent of this function) will iterate again.
    async fn try_connect(&self, addr: Url, last_seen: u64) -> Result<(Url, ChannelPtr)> {
        match self.connector.connect(&addr).await {
            Ok((addr_final, channel)) => Ok((addr_final, channel)),

            Err(err) => {
                info!(
                    target: "net::outbound_session::try_connect()",
                    "[P2P] Unable to connect outbound slot #{} {err}",
                    self.slot
                );

                // Immediately return if the Connector has stopped.
                // This indicates a shutdown of the P2P network and
                // should not result in hostlist modifications.
                if let Error::ConnectorStopped(message) = err {
                    return Err(Error::ConnectFailed(message));
                }

                // At this point we failed to connect. We'll downgrade this peer now.
                self.p2p().hosts().move_host(&addr, last_seen, HostColor::Grey).await?;

                // Mark its state as Suspend, which sends it to the Refinery for processing.
                self.p2p().hosts().try_register(addr.clone(), HostState::Suspend).unwrap();

                // Notify that channel processing failed
                self.p2p().hosts().channel_publisher.notify(Err(err.clone())).await;

                Err(err)
            }
        }
    }

    fn notify(&self) {
        self.wakeup_self.notify()
    }

    fn session(&self) -> OutboundSessionPtr {
        self.session.upgrade().unwrap()
    }
    fn p2p(&self) -> P2pPtr {
        self.session().p2p()
    }
}

/// Defines a common interface for multiple peer discovery processes.
///
/// NOTE: Currently only one Peer Discovery implementation exists. Making
/// Peer Discovery generic enables us to support network swarming, since
/// the peer discovery process will differ depending on whether it occurs
/// on the overlay network or a subnet.
#[async_trait]
pub trait PeerDiscoveryBase {
    async fn start(self: Arc<Self>);

    async fn stop(self: Arc<Self>);

    async fn run(self: Arc<Self>);

    async fn wait(&self) -> bool;

    fn notify(&self);

    fn session(&self) -> OutboundSessionPtr;

    fn p2p(&self) -> P2pPtr;
}

/// Main PeerDiscovery process that loops through connected peers
/// and sends out a `GetAddrs` when it is active. If there are no
/// connected peers after two attempts, connect to our seed nodes
/// and perform `SeedSyncSession`.
struct PeerDiscovery {
    process: StoppableTaskPtr,
    wakeup_self: CondVar,
    session: Weak<OutboundSession>,
}

impl PeerDiscovery {
    fn new(session: Weak<OutboundSession>) -> Arc<Self> {
        Arc::new(Self { process: StoppableTask::new(), wakeup_self: CondVar::new(), session })
    }
}

#[async_trait]
impl PeerDiscoveryBase for PeerDiscovery {
    async fn start(self: Arc<Self>) {
        let ex = self.p2p().executor();
        self.process.clone().start(
            async move {
                self.run().await;
                unreachable!();
            },
            // Ignore stop handler
            |_| async {},
            Error::NetworkServiceStopped,
            ex,
        );
    }
    async fn stop(self: Arc<Self>) {
        self.process.stop().await;
    }

    /// Activate peer discovery if not active already. For the first two
    /// attempts, this will loop through all connected P2P peers and send
    /// out a `GetAddrs` message to request more peers. Other parts of the
    /// P2P stack will then handle the incoming addresses and place them in
    /// the hosts list.  
    ///
    /// On the third attempt, and if we still haven't made any connections,
    /// this function will then call `p2p.seed()` which triggers a
    /// `SeedSyncSession` that will connect to configured seeds and request
    /// peers from them.
    ///
    /// This function will also sleep `outbound_peer_discovery_attempt_time`
    /// seconds after broadcasting in order to let the P2P stack receive and
    /// work through the addresses it is expecting.
    async fn run(self: Arc<Self>) {
        let mut current_attempt = 0;
        loop {
            dnetev!(self, OutboundPeerDiscovery, {
                attempt: current_attempt,
                state: "wait",
            });

            // wait to be woken up by notify()
            let sleep_was_instant = self.wait().await;

            // Read the current P2P settings
            let settings = self.p2p().settings().read_arc().await;
            let outbound_peer_discovery_cooloff_time =
                settings.outbound_peer_discovery_cooloff_time;
            let outbound_peer_discovery_attempt_time =
                settings.outbound_peer_discovery_attempt_time;
            let outbound_connections = settings.outbound_connections;
            let allowed_transports = settings.allowed_transports.clone();
            let seeds = settings.seeds.clone();
            drop(settings);

            if sleep_was_instant {
                // Try again
                current_attempt += 1;
            } else {
                // reset back to start
                current_attempt = 1;
            }

            if current_attempt >= 4 {
                info!(
                    target: "net::outbound_session::peer_discovery()",
                    "[P2P] [PEER DISCOVERY] Sleeping and trying again. Attempt {current_attempt}"
                );

                dnetev!(self, OutboundPeerDiscovery, {
                    attempt: current_attempt,
                    state: "sleep",
                });

                sleep(outbound_peer_discovery_cooloff_time).await;
                current_attempt = 1;
            }

            // First 2 times try sending GetAddr to the network.
            // 3rd time do a seed sync (providing we have seeds
            // configured).
            if self.p2p().is_connected() && current_attempt <= 2 {
                // Broadcast the GetAddrs message to all active peers.
                // If we have no active peers, we will perform a SeedSyncSession instead.
                info!(
                    target: "net::outbound_session::peer_discovery()",
                    "[P2P] [PEER DISCOVERY] Asking peers for new peers to connect to...");

                dnetev!(self, OutboundPeerDiscovery, {
                    attempt: current_attempt,
                    state: "getaddr",
                });

                let get_addrs = GetAddrsMessage {
                    max: outbound_connections as u32,
                    transports: allowed_transports,
                };

                self.p2p().broadcast(&get_addrs).await;

                // Wait for a hosts store update event
                let store_sub = self.p2p().hosts().subscribe_store().await;

                let result = timeout(
                    Duration::from_secs(outbound_peer_discovery_attempt_time),
                    store_sub.receive(),
                )
                .await;

                match result {
                    Ok(addrs_len) => {
                        info!(
                            target: "net::outbound_session::peer_discovery()",
                            "[P2P] [PEER DISCOVERY] Discovered {addrs_len} peers"
                        );
                    }
                    Err(_) => {
                        warn!(
                            target: "net::outbound_session::peer_discovery()",
                            "[P2P] [PEER DISCOVERY] Waiting for addrs timed out."
                        );
                        // Just do seed next time
                        current_attempt = 3;
                    }
                }

                // NOTE: not every call to subscribe() in net/ has a
                // corresponding unsubscribe(). To do this we need async
                // Drop. For now it's sufficient for publishers to be
                // de-allocated when the Session completes.
                store_sub.unsubscribe().await;
            } else if !seeds.is_empty() {
                info!(
                    target: "net::outbound_session::peer_discovery()",
                    "[P2P] [PEER DISCOVERY] Asking seeds for new peers to connect to...");

                dnetev!(self, OutboundPeerDiscovery, {
                    attempt: current_attempt,
                    state: "seed",
                });

                self.p2p().seed().await;
            }

            self.wakeup_self.reset();
            self.session().wakeup_slots().await;

            // Give some time for new connections to be established
            sleep(outbound_peer_discovery_attempt_time).await;
        }
    }

    /// Blocks execution until we receive a notification from notify().
    /// `wakeup_self.wait()` resets the condition variable (`CondVar`) and waits
    /// for a call from `notify()`. Returns `true` if the function completed
    /// instantly (i.e. no wait occured). Returns false otherwise.
    async fn wait(&self) -> bool {
        let wakeup_start = Instant::now();
        self.wakeup_self.wait().await;
        let wakeup_end = Instant::now();

        let epsilon = Duration::from_millis(200);
        wakeup_end - wakeup_start <= epsilon
    }

    /// Wakeup peer discovery by sending a notification to `wakeup_self`.
    /// Uses the underlying `CondVar` method `notify()`. Subsequent calls
    /// to this do nothing until `wait()` is called.
    fn notify(&self) {
        self.wakeup_self.notify()
    }

    fn session(&self) -> OutboundSessionPtr {
        self.session.upgrade().unwrap()
    }

    fn p2p(&self) -> P2pPtr {
        self.session().p2p()
    }
}
