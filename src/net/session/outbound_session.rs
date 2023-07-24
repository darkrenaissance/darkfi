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

use std::collections::HashSet;

use async_std::sync::{Arc, Mutex, Weak};
use async_trait::async_trait;
use log::{debug, error, info};
use smol::Executor;
use url::Url;

use super::{
    super::{
        channel::{ChannelInfo, ChannelPtr},
        connector::Connector,
        message::GetAddrsMessage,
        p2p::{DnetInfo, P2p, P2pPtr},
    },
    Session, SessionBitFlag, SESSION_OUTBOUND,
};
use crate::{
    system::{StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr},
    util::async_util::sleep,
    Error, Result,
};

pub type OutboundSessionPtr = Arc<OutboundSession>;

/// Connection state
#[derive(Eq, PartialEq, Copy, Clone)]
pub enum OutboundState {
    Open,
    Pending,
    Connected,
}

impl std::fmt::Display for OutboundState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Open => "open",
                Self::Pending => "pending",
                Self::Connected => "connected",
            }
        )
    }
}

/// dnet info for an outbound connection
#[derive(Clone)]
pub struct OutboundInfo {
    /// Remote address
    pub addr: Option<Url>,
    /// Channel info
    pub channel: Option<ChannelInfo>,
    /// Connection state
    pub state: OutboundState,
}

impl OutboundInfo {
    async fn dnet_info(&self, p2p: P2pPtr) -> Option<Self> {
        let Some(ref addr) = self.addr else { return None };

        let Some(chan) = p2p.channels().lock().await.get(addr).cloned() else { return None };

        Some(Self {
            addr: self.addr.clone(),
            channel: Some(chan.dnet_info().await),
            state: self.state,
        })
    }
}

impl Default for OutboundInfo {
    fn default() -> Self {
        Self { addr: None, channel: None, state: OutboundState::Open }
    }
}

/// Defines outbound connections session.
pub struct OutboundSession {
    /// Weak pointer to parent p2p object
    p2p: Weak<P2p>,
    /// Outbound connection slots
    connect_slots: Mutex<Vec<StoppableTaskPtr>>,
    /// Subscriber used to signal channels processing
    channel_subscriber: SubscriberPtr<Result<ChannelPtr>>,
    /// Flag to toggle channel_subscriber notifications
    notify: Mutex<bool>,
    /// Channel debug info, corresponds to `connect_slots`
    slot_info: Mutex<Vec<OutboundInfo>>,
}

impl OutboundSession {
    /// Create a new outbound session.
    pub fn new(p2p: Weak<P2p>) -> OutboundSessionPtr {
        Arc::new(Self {
            p2p,
            connect_slots: Mutex::new(vec![]),
            channel_subscriber: Subscriber::new(),
            notify: Mutex::new(false),
            slot_info: Mutex::new(vec![]),
        })
    }

    /// Start the outbound session. Runs the channel connect loop.
    pub async fn start(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        let n_slots = self.p2p().settings().outbound_connections;
        info!(target: "net::outbound_session", "[P2P] Starting {} outbound connection slots.", n_slots);
        // Activate mutex lock on connection slots.
        let mut connect_slots = self.connect_slots.lock().await;

        // Create dnet stub
        self.slot_info.lock().await.resize(n_slots, Default::default());

        for i in 0..n_slots {
            let task = StoppableTask::new();

            task.clone().start(
                self.clone().channel_connect_loop(i, ex.clone()),
                // Ignore stop handler
                |_| async {},
                Error::NetworkServiceStopped,
                ex.clone(),
            );

            connect_slots.push(task);
        }

        Ok(())
    }

    /// Stops the outbound session.
    pub async fn stop(&self) {
        let connect_slots = &*self.connect_slots.lock().await;

        for slot in connect_slots {
            slot.stop().await;
        }
    }

    /// Creates a connector object and tries to connect using it.
    pub async fn channel_connect_loop(
        self: Arc<Self>,
        slot_number: usize,
        ex: Arc<Executor<'_>>,
    ) -> Result<()> {
        let parent = Arc::downgrade(&self);
        let connector = Connector::new(self.p2p().settings(), Arc::new(parent));

        // Retrieve whitelisted outbound transports
        let transports = &self.p2p().settings().allowed_transports;

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
            match self.try_connect(slot_number, &connector, transports, ex.clone()).await {
                Ok(()) => {
                    info!(
                        target: "net::outbound_session",
                        "[P2P] Outbound slot #{} disconnected",
                        slot_number
                    );
                }
                Err(e) => {
                    error!(
                        target: "net::outbound_session",
                        "[P2P] Outbound slot #{} connection failed: {}",
                        slot_number, e,
                    );
                }
            }
        }
    }

    /// Start making an outbound connection, using provided [`Connector`].
    /// Tries to find a valid address to connect to, otherwise does peer
    /// discovery. The peer discovery loops until some peer we can connect
    /// to is found. Once connected, registers the channel, removes it from
    /// the list of pending channels, and starts sending messages across the
    /// channel. In case of any failures, a network error is returned and the
    /// main connect loop (parent of this function) will iterate again.
    async fn try_connect(
        &self,
        slot_number: usize,
        connector: &Connector,
        transports: &[String],
        ex: Arc<Executor<'_>>,
    ) -> Result<()> {
        debug!(
            target: "net::outbound_session::try_connect()",
            "[P2P] Finding a host to connect to for outbound slot #{}",
            slot_number,
        );

        // Find an address to connect to. We also do peer discovery here if needed.
        let addr = self.load_address(slot_number, transports).await?;
        info!(
            target: "net::outbound_session::try_connect()",
            "[P2P] Connecting outbound slot #{} [{}]",
            slot_number, addr,
        );

        match connector.connect(&addr).await {
            Ok((url, channel)) => {
                info!(
                    target: "net::outbound_session::try_connect()",
                    "[P2P] Outbound slot #{} connected [{}]",
                    slot_number, url
                );

                let stop_sub =
                    channel.subscribe_stop().await.expect("Channel should not be stopped");

                // Register the new channel
                self.register_channel(channel.clone(), ex.clone()).await?;

                // Channel is now connected but not yet setup
                // Remove pending lock since register_channel will add the channel to p2p
                self.p2p().remove_pending(&addr).await;

                dnet!(self,
                    let info = &mut self.slot_info.lock().await[slot_number];
                    info.state = OutboundState::Connected;
                );

                // Notify that channel processing has been finished
                if *self.notify.lock().await {
                    self.channel_subscriber.notify(Ok(channel)).await;
                }

                // Wait for channel to close
                stop_sub.receive().await;
                return Ok(())
            }

            Err(e) => {
                error!(
                    target: "net::outbound_session::try_connect()",
                    "[P2P] Unable to connect outbound slot #{} [{}]: {}",
                    slot_number, addr, e
                );
            }
        }

        // At this point we failed to connect. We'll drop this peer now.
        // TODO: We could potentially implement a quarantine zone for this.
        self.p2p().hosts().remove(&addr).await;

        dnet!(self,
            let info = &mut self.slot_info.lock().await[slot_number];
            info.addr = None;
            info.state = OutboundState::Open;
        );

        // Notify that channel processing failed
        if *self.notify.lock().await {
            self.channel_subscriber.notify(Err(Error::ConnectFailed)).await;
        }

        Err(Error::ConnectFailed)
    }

    /// Loops through host addresses to find an outbound address that we can
    /// connect to. Check whether the address is valid by making sure it isn't
    /// our own inbound address, then checks whether it is already connected
    /// (exists) or connecting (pending). If no address was found, we'll attempt
    /// to do peer discovery and try to fill the slot again.
    async fn load_address(&self, slot_number: usize, transports: &[String]) -> Result<Url> {
        loop {
            let p2p = self.p2p();
            let retry_sleep = p2p.settings().outbound_connect_timeout;

            if *p2p.peer_discovery_running.lock().await {
                debug!(
                    target: "net::outbound_session::load_address()",
                    "[P2P] #{} Peer discovery active, waiting {} seconds...",
                    slot_number, retry_sleep,
                );
                sleep(retry_sleep).await;
            }

            // Collect hosts
            let mut hosts = HashSet::new();

            // If transport mixing is enabled, then for example we're allowed to
            // use tor:// to connect to tcp:// and tor+tls:// to connect to tcp+tls://.
            // However, **do not** mix tor:// and tcp+tls://, nor tor+tls:// and tcp://.
            let transport_mixing = self.p2p().settings().transport_mixing;
            macro_rules! mix_transport {
                ($a:expr, $b:expr) => {
                    if transports.contains(&$a.to_string()) && transport_mixing {
                        let mut a_to_b = p2p.hosts().load_with_schemes(&[$b.to_string()]).await;
                        for addr in a_to_b.iter_mut() {
                            addr.set_scheme($a).unwrap();
                            hosts.insert(addr.clone());
                        }
                    }
                };
            }
            mix_transport!("tor", "tcp");
            mix_transport!("tor+tls", "tcp+tls");
            mix_transport!("nym", "tcp");
            mix_transport!("nym+tls", "tcp+tls");

            // And now the actual requested transports
            for addr in p2p.hosts().load_with_schemes(transports).await {
                hosts.insert(addr);
            }

            // Try to find an unused host in the set.
            for host in &hosts {
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

                dnet!(self,
                    let info = &mut self.slot_info.lock().await[slot_number];
                    info.addr = Some(host.clone());
                    info.state = OutboundState::Pending;
                );

                return Ok(host.clone())
            }

            // We didn't find a host to connect to, let's try to find more peers.
            info!(
                target: "net::outbound_session::load_address()",
                "[P2P] Outbound #{}: No peers found. Starting peer discovery...",
                slot_number,
            );
            // NOTE: A design decision here is to do a sleep inside peer_discovery()
            // so that there's a certain period (outbound_connect_timeout) of time
            // to send the GetAddr, receive Addrs, and sort things out. By sleeping
            // inside peer_discovery, it will block here in the slot sessions, while
            // other slots can keep trying to find hosts. This is also why we sleep
            // in the beginning of this loop if peer discovery is currently active.
            self.peer_discovery(slot_number).await;
        }
    }

    /// Activate peer discovery if not active already. This will loop through all
    /// connected P2P channels and send out a `GetAddrs` message to request more
    /// peers. Other parts of the P2P stack will then handle the incoming addresses
    /// and place them in the hosts list.
    /// This function will also sleep `Settings::outbound_connect_timeout` seconds
    /// after broadcasting in order to let the P2P stack receive and work through
    /// the addresses it is expecting.
    async fn peer_discovery(&self, slot_number: usize) {
        let p2p = self.p2p();

        if *p2p.peer_discovery_running.lock().await {
            info!(
                target: "net::outbound_session::peer_discovery()",
                "[P2P] Outbound #{}: Peer discovery already active",
                slot_number,
            );
            return
        }

        info!(
            target: "net::outbound_session::peer_discovery()",
            "[P2P] Outbound #{}: Started peer discovery",
            slot_number,
        );
        *p2p.peer_discovery_running.lock().await = true;

        // Broadcast the GetAddrs message to all active channels
        let get_addrs = GetAddrsMessage { max: p2p.settings().outbound_connections as u32 };
        info!(
            target: "net::outbound_session::peer_discovery()",
            "[P2P] Outbound #{}: Broadcasting GetAddrs across active channels",
            slot_number,
        );
        p2p.broadcast(&get_addrs).await;

        // Now sleep to let the GetAddrs propagate, and hopefully
        // in the meantime we'll get some peers.
        debug!(
            target: "net::outbound_session::peer_discovery()",
            "[P2P] Outbound #{}: Sleeping {} seconds",
            slot_number, p2p.settings().outbound_connect_timeout,
        );
        sleep(p2p.settings().outbound_connect_timeout).await;
        *p2p.peer_discovery_running.lock().await = false;
    }

    /// Enable channel_subscriber notifications.
    pub async fn enable_notify(self: Arc<Self>) {
        *self.notify.lock().await = true;
    }

    /// Disable channel_subscriber notifications.
    pub async fn disable_notify(self: Arc<Self>) {
        *self.notify.lock().await = false;
    }
}

/// Dnet information for the outbound session
pub struct OutboundDnet {
    /// Slot information
    pub slots: Vec<Option<OutboundInfo>>,
}

#[async_trait]
impl Session for OutboundSession {
    fn p2p(&self) -> P2pPtr {
        self.p2p.upgrade().unwrap()
    }

    fn type_id(&self) -> SessionBitFlag {
        SESSION_OUTBOUND
    }

    async fn dnet_info(&self) -> DnetInfo {
        // We fetch channel infos for all outbound slots.
        // If a slot is not connected, it will be `None`.
        let mut slots = vec![];

        for slot in self.slot_info.lock().await.iter() {
            slots.push(slot.dnet_info(self.p2p()).await);
        }

        DnetInfo::Outbound(OutboundDnet { slots })
    }
}
