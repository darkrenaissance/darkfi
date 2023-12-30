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
    time::{Duration, Instant, UNIX_EPOCH},
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
        message::GetAddrsMessage,
        p2p::{P2p, P2pPtr},
    },
    Session, SessionBitFlag, SESSION_OUTBOUND,
};
use crate::{
    system::{
        sleep, timeout::timeout, CondVar, LazyWeak, StoppableTask, StoppableTaskPtr, Subscriber,
        SubscriberPtr,
    },
    Error, Result,
};

pub type OutboundSessionPtr = Arc<OutboundSession>;

/// Defines outbound connections session.
pub struct OutboundSession {
    /// Weak pointer to parent p2p object
    pub(in crate::net) p2p: LazyWeak<P2p>,
    /// Subscriber used to signal channels processing
    channel_subscriber: SubscriberPtr<Result<ChannelPtr>>,

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
            channel_subscriber: Subscriber::new(),
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

pub struct Slot {
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
        // TODO: way too many clones, look into making this implicit. See implicit-clone crate
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

    // We first try to make connections to the addresses on our anchor list. We then find some
    // whitelist connections according to the whitelist percent default. Finally, any remaining
    // connections we make from the greylist.
    async fn run(self: Arc<Self>) {
        let hosts = self.p2p().hosts();
        let slot_count = self.p2p().settings().outbound_connections;
        let white_count = slot_count * self.p2p().settings().white_connection_percent / 100;

        loop {
            // Activate the slot
            debug!(
                target: "net::outbound_session::try_connect()",
                "[P2P] Finding a host to connect to for outbound slot #{}",
                self.slot,
            );

            // Retrieve outbound transports
            let transports = &self.p2p().settings().allowed_transports;

            // Get the active connection count.
            let connect_count = self.get_active_connect_count().await;

            // Do peer discovery if we don't have a hostlist (first time connecting
            // to the network).
            if hosts.is_empty_hostlist().await {
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

            // Uo to anchor_connection_count connections:
            //
            //  Select from the anchorlist
            //  If the anchorlist is empty, select from the whitelist
            //  If the whitelist is empty, select from the greylist
            //  If the greylist is empty, do peer discovery
            //
            // Up to white_connection_percent connections:
            //
            //  Select from the whitelist
            //  If the whitelist is empty, select from the greylist
            //  If the greylist is empty, do peer discovery
            //
            // All other connections:
            //
            //  Select from the greylist
            //  If the greylist is empty, do peer discovery

            if connect_count < self.p2p().settings().anchor_connection_count {
                match hosts.anchorlist_fetch_address_with_lock(self.p2p(), transports).await {
                    Some(host) => {
                        // Connect to whitelist addr
                        self.connect_slot(&host.0, self.slot).await.unwrap();
                    }
                    None => {
                        // We haven't been able to connect to any known peers. Activate peer discovery.
                        dnetev!(self, OutboundSlotSleeping, {
                        slot: self.slot,
                        });

                        self.wakeup_self.reset();
                        // Peer discovery
                        self.session().wakeup_peer_discovery();
                        // Wait to be woken up by peer discovery
                        self.wakeup_self.wait().await;
                    }
                }
            }

            if connect_count < white_count {
                // Take from the greylist if there's nothing on the whitelist.
                match hosts.whitelist_fetch_address_with_lock(self.p2p(), transports).await {
                    Some(host) => self.connect_slot(&host.0, self.slot).await.unwrap(),
                    None => {
                        // We haven't been able to connect to any known peers. Activate peer discovery.
                        dnetev!(self, OutboundSlotSleeping, {
                            slot: self.slot,
                        });

                        self.wakeup_self.reset();
                        // Peer discovery
                        self.session().wakeup_peer_discovery();
                        // Wait to be woken up by peer discovery
                        self.wakeup_self.wait().await;
                    }
                }
            }

            // For any remaining slots, connect to a host on the greylist.
            if connect_count < slot_count {
                match hosts.greylist_fetch_address_with_lock(self.p2p(), transports).await {
                    Some(host) => {
                        self.connect_slot(&host.0, self.slot).await.unwrap();
                    }

                    None => {
                        // We haven't been able to connect to any known peers. Activate peer discovery.
                        dnetev!(self, OutboundSlotSleeping, {
                            slot: self.slot,
                        });

                        self.wakeup_self.reset();
                        // Peer discovery
                        self.session().wakeup_peer_discovery();
                        // Wait to be woken up by peer discovery
                        self.wakeup_self.wait().await;
                    }
                }
            }
        }
    }

    async fn connect_slot(&self, host: &Url, slot: u32) -> Result<()> {
        info!(
            target: "net::outbound_session::try_connect()",
            "[P2P] Connecting outbound slot #{} [{}]",
            slot, host,
        );

        dnetev!(self, OutboundSlotConnecting, {
            slot: slot,
            addr: host.clone(),
        });

        let (addr, channel) = match self.try_connect(host.clone()).await {
            Ok(connect_info) => connect_info,
            Err(err) => {
                error!(
                    target: "net::outbound_session",
                    "[P2P] Outbound slot #{} connection failed: {}",
                    slot, err,
                );

                dnetev!(self, OutboundSlotDisconnected, {
                    slot,
                    err: err.to_string()
                });

                self.channel_id.store(0, Ordering::Relaxed);
                return Err(err.into())
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

        let stop_sub = channel.subscribe_stop().await.expect("Channel should not be stopped");
        // Setup new channel
        if let Err(err) = self.setup_channel(host.clone(), channel.clone()).await {
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
            return Err(err.into())
        }

        self.channel_id.store(channel.info.id, Ordering::Relaxed);

        // Wait for channel to close
        stop_sub.receive().await;
        self.channel_id.store(0, Ordering::Relaxed);

        Ok(())
    }

    /// Start making an outbound connection, using provided [`Connector`].
    /// Tries to find a valid address to connect to, otherwise does peer
    /// discovery. The peer discovery loops until some peer we can connect
    /// to is found. Once connected, registers the channel, removes it from
    /// the list of pending channels, and starts sending messages across the
    /// channel. In case of any failures, a network error is returned and the
    /// main connect loop (parent of this function) will iterate again.
    async fn try_connect(&self, addr: Url) -> Result<(Url, ChannelPtr)> {
        let parent = Arc::downgrade(&self.session());
        let connector = Connector::new(self.p2p().settings(), parent);

        match connector.connect(&addr).await {
            Ok((addr_final, channel)) => Ok((addr_final, channel)),

            Err(e) => {
                error!(
                    target: "net::outbound_session::try_connect()",
                    "[P2P] Unable to connect outbound slot #{} [{}]: {}",
                    self.slot, addr, e
                );

                // Remove connection from pending
                self.p2p().remove_pending(&addr).await;

                // Notify that channel processing failed
                self.session().channel_subscriber.notify(Err(Error::ConnectFailed)).await;

                Err(Error::ConnectFailed)
            }
        }
    }

    async fn setup_channel(&self, addr: Url, channel: ChannelPtr) -> Result<()> {
        // Register the new channel
        debug!(target: "net::outbound_session::setup_channel", "register_channel {}", channel.clone().address());
        self.session().register_channel(channel.clone(), self.p2p().executor()).await?;

        // Channel is now connected but not yet setup
        // Remove pending lock since register_channel will add the channel to p2p
        self.p2p().remove_pending(&addr).await;

        // Notify that channel processing has been finished
        self.session().channel_subscriber.notify(Ok(channel)).await;

        Ok(())
    }

    async fn get_active_connect_count(&self) -> usize {
        let mut connect_count = 0;
        for channel in self.p2p().channels().await {
            if channel.session_type_id() == SESSION_OUTBOUND {
                connect_count += 1;
            }
        }
        connect_count
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
    /// This function will also sleep `Settings::outbound_connect_timeout` seconds
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
                let store_sub = self.p2p().hosts().subscribe_store().await.unwrap();

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
