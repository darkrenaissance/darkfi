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
    system::{sleep, timeout::timeout, CondVar, LazyWeak, StoppableTask, StoppableTaskPtr},
    Error, Result,
};

pub type OutboundSessionPtr = Arc<OutboundSession>;

/// Defines outbound connections session.
pub struct OutboundSession {
    /// Weak pointer to parent p2p object
    pub(in crate::net) p2p: LazyWeak<P2p>,
    /// Outbound connection slots
    slots: Mutex<Vec<Arc<Slot>>>,
    /// Peer discovery task
    peer_discovery: Arc<PeerDiscovery>,
}

impl OutboundSession {
    /// Create a new outbound session.
    pub(crate) fn new() -> OutboundSessionPtr {
        let self_ = Arc::new(Self {
            p2p: LazyWeak::new(),
            slots: Mutex::new(Vec::new()),
            peer_discovery: PeerDiscovery::new(),
        });
        self_.peer_discovery.session.init(self_.clone());
        self_
    }

    /// Start the outbound session. Runs the channel connect loop.
    pub(crate) async fn start(self: Arc<Self>) {
        let n_slots = self.p2p().settings().outbound_connections;
        info!(target: "net::outbound_session", "[P2P] Starting {} outbound connection slots.", n_slots);
        // Activate mutex lock on connection slots.
        let mut slots = self.slots.lock().await;

        let self_ = Arc::downgrade(&self);

        for i in 0..n_slots as u32 {
            let slot = Slot::new(self_.clone(), i);
            slot.clone().start().await;
            slots.push(slot);
        }

        self.peer_discovery.clone().start().await;
    }

    /// Stops the outbound session.
    pub(crate) async fn stop(&self) {
        debug!(target: "net::outbound_session", "Stopping outbound session");
        let slots = &*self.slots.lock().await;

        for slot in slots {
            slot.clone().stop().await;
        }

        self.peer_discovery.clone().stop().await;
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
        self.p2p.upgrade()
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
    // For debugging
    channel_id: AtomicU32,
}

impl Slot {
    fn new(session: Weak<OutboundSession>, slot: u32) -> Arc<Self> {
        Arc::new(Self {
            slot,
            process: StoppableTask::new(),
            wakeup_self: CondVar::new(),
            session,
            channel_id: AtomicU32::new(0),
        })
    }

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
        self.process.stop().await
    }

    /// Address selection algorithm that works as follows: up to
    /// anchor_count, select from the anchorlist. Up to white_count,
    /// select from the whitelist. For all other slots, select from
    /// the greylist.
    ///
    /// If we didn't find an address with this selection logic, downgrade
    /// our preferences. Up to anchor_count, select from the whitelist,
    /// up until white_count, select from the greylist.
    ///
    /// If we still didn't find an address, select from the greylist. In
    /// all other cases, return an empty vector. This will trigger
    /// fetch_addrs() to return None and initiate peer discovery.
    /* NOTE: Selecting from the greylist for some % of the slots is
    necessary and healthy since we require the network retains some
    unreliable connections. A network that purely favors uptime over
    unreliable connections may be vulnerable to sybil by attackers with
    good uptime.*/
    async fn fetch_addrs_with_preference(&self, preference: usize) -> Vec<(Url, u64)> {
        let slot = self.slot;
        let settings = self.p2p().settings();
        let hosts = &self.p2p().hosts().container;

        let white_count = settings.white_connect_count;
        let anchor_count = settings.anchor_connect_count;

        let transports = &settings.allowed_transports;
        let transport_mixing = settings.transport_mixing;

        debug!(target: "net::outbound_session::fetch_addrs_with_preference()",
        "slot={}, preference={}", slot, preference);

        match preference {
            // Highest preference that corresponds to the anchor and white count preference set in
            // Settings.
            0 => {
                if slot < anchor_count {
                    hosts.fetch(HostColor::Gold, transports, transport_mixing).await
                } else if slot < white_count {
                    hosts.fetch(HostColor::White, transports, transport_mixing).await
                } else {
                    hosts.fetch(HostColor::Grey, transports, transport_mixing).await
                }
            }
            // Reduced preference in case we don't have sufficient hosts to satisfy our highest
            // preference.
            1 => {
                if slot < anchor_count {
                    hosts.fetch(HostColor::White, transports, transport_mixing).await
                } else if slot < white_count {
                    hosts.fetch(HostColor::Grey, transports, transport_mixing).await
                } else {
                    vec![]
                }
            }
            // Lowest preference if we still haven't been able to find a host.
            2 => {
                if slot < anchor_count {
                    hosts.fetch(HostColor::Grey, transports, transport_mixing).await
                } else {
                    vec![]
                }
            }
            _ => {
                panic!()
            }
        }
    }

    // Fetch an address we can connect to acccording to the white and anchor connection counts
    // configured in Settings.
    async fn fetch_addrs(&self) -> Option<(Url, u64)> {
        let hosts = self.p2p().hosts();

        // First select an addresses that match our white and anchor requirements configured in
        // Settings.
        let preference = 0;
        let addrs = self.fetch_addrs_with_preference(preference).await;

        if !addrs.is_empty() {
            return hosts.check_addrs(addrs).await;
        }

        // If no addresses were returned, go for the second best thing (white and grey).
        let preference = 1;
        let addrs = self.fetch_addrs_with_preference(preference).await;

        if !addrs.is_empty() {
            return hosts.check_addrs(addrs).await;
        }

        // If we still have no addresses, go for the least favored option.
        let preference = 2;
        let addrs = self.fetch_addrs_with_preference(preference).await;

        if !addrs.is_empty() {
            return hosts.check_addrs(addrs).await;
        }

        // If we still don't have an address, return None and do peer discovery.
        None
    }

    // We first try to make connections to the addresses on our anchor list. We then find some
    // whitelist connections according to the whitelist percent default. Finally, any remaining
    // connections we make from the greylist.
    async fn run(self: Arc<Self>) {
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
            if hosts.container.is_empty(HostColor::Grey).await &&
                hosts.container.is_empty(HostColor::White).await &&
                hosts.container.is_empty(HostColor::Gold).await
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
                debug!(target: "net::outbound_session::run()", "Fetched address: {:?}", addr);
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
                "[P2P] Connecting outbound slot #{} [{}]",
                slot, host,
            );

            dnetev!(self, OutboundSlotConnecting, {
                slot,
                addr: host.clone(),
            });

            let (addr, channel) = match self.try_connect(host.clone(), last_seen).await {
                Ok(connect_info) => connect_info,
                Err(err) => {
                    debug!(
                        target: "net::outbound_session::try_connect()",
                        "[P2P] Outbound slot #{} connection failed: {}",
                        slot, err
                    );

                    dnetev!(self, OutboundSlotDisconnected, {
                        slot,
                        err: err.to_string()
                    });

                    self.channel_id.store(0, Ordering::Relaxed);
                    continue
                }
            };

            info!(
                target: "net::outbound_session::try_connect()",
                "[P2P] Outbound slot #{} connected [{}]",
                slot, addr
            );

            dnetev!(self, OutboundSlotConnected, {
                slot: self.slot,
                addr: addr.clone(),
                channel_id: channel.info.id
            });

            // At this point we've managed to connect.

            let stop_sub = channel.subscribe_stop().await.expect("Channel should not be stopped");
            // Setup new channel
            if let Err(err) =
                self.session().register_channel(channel.clone(), self.p2p().executor()).await
            {
                info!(
                    target: "net::outbound_session",
                    "[P2P] Outbound slot #{} disconnected: {}",
                    slot, err
                );

                dnetev!(self, OutboundSlotDisconnected, {
                    slot: self.slot,
                    err: err.to_string()
                });

                self.channel_id.store(0, Ordering::Relaxed);
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
        let parent = Arc::downgrade(&self.session());
        let connector = Connector::new(self.p2p().settings(), parent);

        match connector.connect(&addr).await {
            Ok((addr_final, channel)) => Ok((addr_final, channel)),

            Err(e) => {
                debug!(
                    target: "net::outbound_session::try_connect()",
                    "[P2P] Unable to connect outbound slot #{} [{}]: {}",
                    self.slot, addr, e
                );

                // At this point we failed to connect. We'll downgrade this peer now.
                self.p2p().hosts().move_host(&addr, last_seen, HostColor::Grey).await?;

                // Mark its state as Suspend, which sends it to the Refinery for processing.
                self.p2p().hosts().try_register(addr.clone(), HostState::Suspend).await.unwrap();

                // Notify that channel processing failed
                self.p2p().hosts().channel_subscriber.notify(Err(Error::ConnectFailed)).await;

                Err(Error::ConnectFailed)
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
/* NOTE: Currently only one Peer Discovery implementation exists. Making
Peer Discovery generic enables us to support network swarming, since
the peer discovery process will differ depending on whether it occurs
on the overlay network or a subnet.*/
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

/// Main PeerDiscovery process that loops through connected channels
/// and sends out a `GetAddrs` when it is active.
struct PeerDiscovery {
    process: StoppableTaskPtr,
    wakeup_self: CondVar,
    session: LazyWeak<OutboundSession>,
}

impl PeerDiscovery {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            process: StoppableTask::new(),
            wakeup_self: CondVar::new(),
            session: LazyWeak::new(),
        })
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
        self.process.stop().await
    }

    /// Activate peer discovery if not active already. This will loop through all
    /// connected P2P channels and send out a `GetAddrs` message to request more
    /// peers. Other parts of the P2P stack will then handle the incoming addresses
    /// and place them in the hosts list.
    /// This function will also sleep `Settings::outbound_peer_discovery_attempt_time` seconds
    /// after broadcasting in order to let the P2P stack receive and work through
    /// the addresses it is expecting.
    async fn run(self: Arc<Self>) {
        let mut current_attempt = 0;
        loop {
            dnetev!(self, OutboundPeerDiscovery, {
                attempt: current_attempt,
                state: "wait",
            });

            // wait to be woken up by notify()
            let sleep_was_instant = self.wait().await;

            let p2p = self.p2p();

            if sleep_was_instant {
                // Try again
                current_attempt += 1;
            } else {
                // reset back to start
                current_attempt = 1;
            }

            if current_attempt >= 4 {
                debug!("current attempt: {}", current_attempt);
                info!(
                    target: "net::outbound_session::peer_discovery()",
                    "[P2P] Sleeping and trying again..."
                );

                dnetev!(self, OutboundPeerDiscovery, {
                    attempt: current_attempt,
                    state: "sleep",
                });

                sleep(p2p.settings().outbound_peer_discovery_cooloff_time).await;
                current_attempt = 1;
            }

            // First 2 times try sending GetAddr to the network.
            // 3rd time do a seed sync.
            if p2p.is_connected().await && current_attempt <= 2 {
                // Broadcast the GetAddrs message to all active channels.
                // If we have no active channels, we will perform a SeedSyncSession instead.

                info!(
                    target: "net::outbound_session::peer_discovery()",
                    "[P2P] Requesting addrs from active channels. Attempt: {}",
                    current_attempt
                );

                dnetev!(self, OutboundPeerDiscovery, {
                    attempt: current_attempt,
                    state: "getaddr",
                });

                let get_addrs = GetAddrsMessage {
                    max: p2p.settings().outbound_connections as u32,
                    transports: p2p.settings().allowed_transports.clone(),
                };
                p2p.broadcast(&get_addrs).await;

                // Wait for a hosts store update event
                let store_sub = self.p2p().hosts().subscribe_store().await;

                let result = timeout(
                    Duration::from_secs(p2p.settings().outbound_peer_discovery_attempt_time),
                    store_sub.receive(),
                )
                .await;

                match result {
                    Ok(addrs_len) => {
                        info!(
                            target: "net::outbound_session::peer_discovery()",
                            "[P2P] Discovered {} addrs", addrs_len
                        );
                    }
                    Err(_) => {
                        warn!(
                            target: "net::outbound_session::peer_discovery()",
                            "[P2P] Peer discovery waiting for addrs timed out."
                        );
                        // TODO: Just do seed next time
                    }
                }

                // TODO: check every subscribe() call has a corresponding unsubscribe()
                store_sub.unsubscribe().await;
            } else {
                info!(
                    target: "net::outbound_session::peer_discovery()",
                    "[P2P] Seeding hosts. Attempt: {}",
                    current_attempt
                );

                dnetev!(self, OutboundPeerDiscovery, {
                    attempt: current_attempt,
                    state: "seed",
                });

                match p2p.clone().seed().await {
                    Ok(()) => {
                        info!(
                            target: "net::outbound_session::peer_discovery()",
                            "[P2P] Seeding hosts successful."
                        );
                    }
                    Err(err) => {
                        error!(
                            target: "net::outbound_session::peer_discovery()",
                            "[P2P] Network reseed failed: {}", err,
                        );
                    }
                }
            }

            self.wakeup_self.reset();
            self.session().wakeup_slots().await;

            // Give some time for new connections to be established
            sleep(p2p.settings().outbound_peer_discovery_attempt_time).await;
        }
    }

    async fn wait(&self) -> bool {
        let wakeup_start = Instant::now();
        self.wakeup_self.wait().await;
        let wakeup_end = Instant::now();

        let epsilon = Duration::from_millis(200);
        wakeup_end - wakeup_start <= epsilon
    }

    fn notify(&self) {
        self.wakeup_self.notify()
    }
    fn session(&self) -> OutboundSessionPtr {
        self.session.upgrade()
    }

    fn p2p(&self) -> P2pPtr {
        self.session().p2p()
    }
}
