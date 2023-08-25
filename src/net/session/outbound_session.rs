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

use std::sync::{Arc, Weak};

use async_trait::async_trait;
use log::{debug, error, info, warn};
use rand::{prelude::SliceRandom, rngs::OsRng};
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
        sleep, CondVar, LazyWeak, StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr,
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
}

impl Slot {
    fn new(session: Weak<OutboundSession>, slot: u32) -> Arc<Self> {
        Arc::new(Self { slot, process: StoppableTask::new(), wakeup_self: CondVar::new(), session })
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

    async fn run(self: Arc<Self>) {
        // This is the main outbound connection loop where we try to establish
        // a connection in the slot. The `try_connect` function will block in
        // case the connection was sucessfully established. If it fails, then
        // we will wait for a defined number of seconds and try to fill the
        // slot again. This function should never exit during the lifetime of
        // the P2P network, as it is supposed to represent an outbound slot we
        // want to fill.
        // The actual connection logic and peer selection is in `try_connect`.
        // If the connection is successful, `try_connect` will wait for a stop
        // signal and then exit. Once it exits, we'll run `try_connect` again
        // and attempt to fill the slot with another peer.
        loop {
            // Activate the slot
            debug!(
                target: "net::outbound_session::try_connect()",
                "[P2P] Finding a host to connect to for outbound slot #{}",
                self.slot,
            );

            // Retrieve whitelisted outbound transports
            let transports = &self.p2p().settings().allowed_transports;

            // Find an address to connect to. We also do peer discovery here if needed.
            let addr = if let Some(addr) = self.fetch_address_with_lock(transports).await {
                addr
            } else {
                self.wakeup_self.reset();
                // Peer discovery
                self.session().wakeup_peer_discovery();
                // Wait to be woken up by peer discovery
                self.wakeup_self.wait().await;
                continue
            };

            let (addr_final, channel) = match self.try_connect(addr.clone()).await {
                Ok(connect_info) => connect_info,
                Err(err) => {
                    error!(
                        target: "net::outbound_session",
                        "[P2P] Outbound slot #{} connection failed: {}",
                        self.slot, err,
                    );

                    dnetev!(self, OutboundDisconnected, {
                        slot: self.slot,
                        err: err.to_string()
                    });
                    continue
                }
            };

            let stop_sub = channel.subscribe_stop().await.expect("Channel should not be stopped");
            // Setup new channel
            if let Err(err) = self.setup_channel(addr, addr_final, channel.clone()).await {
                info!(
                    target: "net::outbound_session",
                    "[P2P] Outbound slot #{} disconnected: {}",
                    self.slot, err
                );

                dnetev!(self, OutboundDisconnected, {
                    slot: self.slot,
                    err: err.to_string()
                });
                continue
            }
            // Wait for channel to close
            stop_sub.receive().await;
        }
    }

    /// Start making an outbound connection, using provided [`Connector`].
    /// Tries to find a valid address to connect to, otherwise does peer
    /// discovery. The peer discovery loops until some peer we can connect
    /// to is found. Once connected, registers the channel, removes it from
    /// the list of pending channels, and starts sending messages across the
    /// channel. In case of any failures, a network error is returned and the
    /// main connect loop (parent of this function) will iterate again.
    async fn try_connect(&self, addr: Url) -> Result<(Url, ChannelPtr)> {
        info!(
            target: "net::outbound_session::try_connect()",
            "[P2P] Connecting outbound slot #{} [{}]",
            self.slot, addr,
        );

        dnetev!(self, OutboundConnecting, {
            slot: self.slot,
            addr: addr.clone(),
        });

        let parent = Arc::downgrade(&self.session());
        let connector = Connector::new(self.p2p().settings(), Arc::new(parent));

        match connector.connect(&addr).await {
            Ok((addr_final, channel)) => Ok((addr_final, channel)),

            Err(e) => {
                error!(
                    target: "net::outbound_session::try_connect()",
                    "[P2P] Unable to connect outbound slot #{} [{}]: {}",
                    self.slot, addr, e
                );

                // At this point we failed to connect. We'll quarantine this peer now.
                self.p2p().hosts().quarantine(&addr).await;

                // Notify that channel processing failed
                self.session().channel_subscriber.notify(Err(Error::ConnectFailed)).await;

                Err(Error::ConnectFailed)
            }
        }
    }

    async fn setup_channel(&self, addr: Url, addr_final: Url, channel: ChannelPtr) -> Result<()> {
        info!(
            target: "net::outbound_session::try_connect()",
            "[P2P] Outbound slot #{} connected [{}]",
            self.slot, addr_final
        );

        dnetev!(self, OutboundConnected, {
            slot: self.slot,
            addr: addr_final.clone(),
            channel_id: channel.info.id
        });

        // Register the new channel
        self.session().register_channel(channel.clone(), self.p2p().executor()).await?;

        // Channel is now connected but not yet setup
        // Remove pending lock since register_channel will add the channel to p2p
        self.p2p().remove_pending(&addr).await;

        // Notify that channel processing has been finished
        self.session().channel_subscriber.notify(Ok(channel)).await;

        Ok(())
    }

    /// Loops through host addresses to find an outbound address that we can
    /// connect to. Check whether the address is valid by making sure it isn't
    /// our own inbound address, then checks whether it is already connected
    /// (exists) or connecting (pending).
    /// Lastly adds matching address to the pending list.
    /// TODO: this method should go in hosts
    async fn fetch_address_with_lock(&self, transports: &[String]) -> Option<Url> {
        let p2p = self.p2p();

        // Collect hosts
        let mut hosts = vec![];

        // If transport mixing is enabled, then for example we're allowed to
        // use tor:// to connect to tcp:// and tor+tls:// to connect to tcp+tls://.
        // However, **do not** mix tor:// and tcp+tls://, nor tor+tls:// and tcp://.
        let transport_mixing = p2p.settings().transport_mixing;
        macro_rules! mix_transport {
            ($a:expr, $b:expr) => {
                if transports.contains(&$a.to_string()) && transport_mixing {
                    let mut a_to_b = p2p.hosts().fetch_with_schemes(&[$b.to_string()]).await;
                    for addr in a_to_b.iter_mut() {
                        addr.set_scheme($a).unwrap();
                        hosts.push(addr.clone());
                    }
                }
            };
        }
        mix_transport!("tor", "tcp");
        mix_transport!("tor+tls", "tcp+tls");
        mix_transport!("nym", "tcp");
        mix_transport!("nym+tls", "tcp+tls");

        // And now the actual requested transports
        for addr in p2p.hosts().fetch_with_schemes(transports).await {
            hosts.push(addr);
        }

        // Randomize hosts list. Do not try to connect in a deterministic order.
        // This is healthier for multiple slots to not compete for the same addrs.
        hosts.shuffle(&mut OsRng);

        // Try to find an unused host in the set.
        for host in hosts.iter() {
            // Check if we already have this connection established
            if p2p.exists(host).await {
                continue
            }

            // Check if we already have this configured as a manual peer
            if p2p.settings().peers.contains(host) {
                continue
            }

            // Obtain a lock on this address to prevent duplicate connection
            if !p2p.add_pending(host).await {
                continue
            }

            return Some(host.clone())
        }

        None
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
        loop {
            // wait to be woken up by notify()
            self.wakeup_self.wait().await;

            let p2p = self.p2p();

            // Broadcast the GetAddrs message to all active channels.
            // If we have no active channels, we will perform a SeedSyncSession instead.
            if p2p.is_connected().await {
                info!(
                    target: "net::outbound_session::peer_discovery()",
                    "[P2P] Outbound: Broadcasting GetAddrs across active channels",
                );

                let get_addrs = GetAddrsMessage { max: p2p.settings().outbound_connections as u32 };
                p2p.broadcast(&get_addrs).await;
                // Temporary workaround. Sleep until the nodes respond back and
                // we process the addr messages.
                sleep(p2p.settings().outbound_connect_timeout).await;
            } else {
                warn!(
                    target: "net::outbound_session::peer_discovery()",
                    "[P2P] No connected channels found for peer discovery. Reseeding.",
                );

                if let Err(e) = p2p.clone().seed().await {
                    error!(
                        target: "net::outbound_session::peer_discovery()",
                        "[P2P] Network reseed failed: {}", e,
                    );
                }
            }

            self.session().wakeup_slots().await;
            self.wakeup_self.reset();
        }
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
