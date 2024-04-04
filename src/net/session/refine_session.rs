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

//! `RefineSession` manages two processes, the `GreylistRefinery`, which
//! periodically pings entries on the greylist and updates them to whitelist
//! if active, and `SelfHandshake`, which periodically pings our own external
//! addresses to ensure they are active before broadcasting to the network.
//!
//! Both processes make use of a `RefineSession` method called
//! `handshake_node()`, which uses a `Connector` to establish a `Channel` with
//! a provided address, and then does a version exchange across the channel
//! (`perform_handshake_protocols`). `handshake_node()` can either succeed,
//! fail, or timeout.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, UNIX_EPOCH},
};

use async_trait::async_trait;
use log::{debug, warn};
use smol::lock::Mutex;
use url::Url;

use super::super::p2p::{P2p, P2pPtr};

use crate::{
    net::{
        connector::Connector,
        hosts::{HostColor, HostState},
        protocol::ProtocolVersion,
        session::{Session, SessionBitFlag, SESSION_REFINE},
    },
    system::{sleep, timeout::timeout, LazyWeak, StoppableTask, StoppableTaskPtr},
    Error,
};

pub type RefineSessionPtr = Arc<RefineSession>;

pub struct RefineSession {
    /// Weak pointer to parent p2p object
    pub(in crate::net) p2p: LazyWeak<P2p>,

    /// Task that periodically checks entries in the greylist.
    pub(in crate::net) refinery: Arc<GreylistRefinery>,

    /// Task that periodically checks our external addresses.
    pub(in crate::net) self_handshake: Arc<SelfHandshake>,
}

impl RefineSession {
    pub fn new() -> RefineSessionPtr {
        let self_ = Arc::new(Self {
            p2p: LazyWeak::new(),
            refinery: GreylistRefinery::new(),
            self_handshake: SelfHandshake::new(),
        });
        self_.self_handshake.session.init(self_.clone());
        self_.refinery.session.init(self_.clone());
        self_
    }

    /// Start the refinery and self handshake processes.
    pub(crate) async fn start(self: Arc<Self>) {
        match self.p2p().hosts().container.load_all(&self.p2p().settings().hostlist).await {
            Ok(()) => {
                debug!(target: "net::refine_session::start()", "Load hosts successful!");
            }
            Err(e) => {
                warn!(target: "net::refine_session::start()", "Error loading hosts {}", e);
            }
        }
        match self.p2p().hosts().import_blacklist().await {
            Ok(()) => {
                debug!(target: "net::refine_session::start()", "Import blacklist successful!");
            }
            Err(e) => {
                warn!(target: "net::refine_session::start()",
                    "Error importing blacklist from config file {}", e);
            }
        }

        debug!(target: "net::refine_session", "Starting greylist refinery process");
        self.refinery.clone().start().await;

        debug!(target: "net::refine_session", "Starting self handshake process");
        self.self_handshake.clone().start().await;
    }

    /// Stop the refinery and self handshake processes.
    pub(crate) async fn stop(&self) {
        match self.p2p().hosts().container.save_all(&self.p2p().settings().hostlist).await {
            Ok(()) => {
                debug!(target: "net::refine_session::stop()", "Save hosts successful!");
            }
            Err(e) => {
                warn!(target: "net::refine_session::stop()", "Error saving hosts {}", e);
            }
        }

        debug!(target: "net::refine_session", "Stopping refinery process");
        self.refinery.clone().stop().await;

        debug!(target: "net::refine_session", "Stopping self handshake process");
        self.self_handshake.clone().stop().await;
    }

    /// Globally accessible function to perform a version exchange with a
    /// given address.  Returns `true` if an address is accessible, false
    /// otherwise.  Used by `GreylistRefinery`, `SelfHandshake`, and in
    /// `Lilith`, which contains an implemenenation of a whitelist refinery.
    pub async fn handshake_node(self: Arc<Self>, addr: Url, p2p: P2pPtr) -> bool {
        let self_ = Arc::downgrade(&self);
        let connector = Connector::new(self.p2p().settings(), self_);

        debug!(target: "net::refinery::handshake_node()", "Attempting to connect to {}", addr);
        match connector.connect(&addr).await {
            Ok((url, channel)) => {
                debug!(target: "net::refinery::handshake_node()", "Successfully created a channel with {}", url);
                // First initialize the version protocol and its Version, Verack subscribers.
                let proto_ver = ProtocolVersion::new(channel.clone(), p2p.settings()).await;

                debug!(target: "net::refinery::handshake_node()", "Performing handshake protocols with {}", url);
                // Then run the version exchange, store the channel and subscribe to a stop signal.
                let handshake_task =
                    self.perform_handshake_protocols(proto_ver, channel.clone(), p2p.executor());

                debug!(target: "net::refinery::handshake_node()", "Starting channel {}", url);
                channel.clone().start(p2p.executor());

                // Ensure the channel gets stopped by adding a timeout to the handshake. Otherwise if
                // the handshake does not finish channel.stop() will never get called, resulting in
                // zombie processes.
                let result = timeout(Duration::from_secs(5), handshake_task).await;

                debug!(target: "net::refinery::handshake_node()", "Stopping channel {}", url);
                channel.stop().await;

                match result {
                    Ok(_) => {
                        debug!(target: "net::refinery::handshake_node()", "Handshake success!");
                        true
                    }
                    Err(e) => {
                        debug!(target: "net::refinery::handshake_node()", "Handshake err: {}", e);
                        false
                    }
                }
            }

            Err(e) => {
                debug!(target: "net::refinery::handshake_node()", "Failed to connect to {}, ({})", addr, e);
                false
            }
        }
    }
}

#[async_trait]
impl Session for RefineSession {
    fn p2p(&self) -> P2pPtr {
        self.p2p.upgrade()
    }

    fn type_id(&self) -> SessionBitFlag {
        SESSION_REFINE
    }
}

/// Periodically probes entries in the greylist.
///
/// Randomly selects a greylist entry and tries to establish a local
/// connection to it using the method handshake_node(), which creates a
/// channel and does a version exchange using `perform_handshake_protocols()`.
///
/// If successful, the entry is removed from the greylist and added to the
/// whitelist with an updated last_seen timestamp. If non-successful, the
/// entry is removed from the greylist.
pub struct GreylistRefinery {
    /// Weak pointer to parent object
    session: LazyWeak<RefineSession>,
    process: StoppableTaskPtr,
}

impl GreylistRefinery {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { session: LazyWeak::new(), process: StoppableTask::new() })
    }

    pub async fn start(self: Arc<Self>) {
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

    pub async fn stop(self: Arc<Self>) {
        debug!(target: "net::refinery", "Stopping refinery");
        self.process.stop().await;
    }

    // Randomly select a peer on the greylist and probe it. This method will remove from the
    // greylist and store on the whitelist providing the peer is responsive.
    async fn run(self: Arc<Self>) {
        let settings = self.p2p().settings();
        let hosts = self.p2p().hosts();
        loop {
            sleep(settings.greylist_refinery_interval).await;

            if hosts.container.is_empty(HostColor::Grey).await {
                debug!(target: "net::refinery",
                "Greylist is empty! Cannot start refinery process");

                continue
            }

            // Pause the refinery if we've had zero connections for longer than the configured
            // limit.
            let offline_limit = Duration::from_secs(settings.time_with_no_connections);
            let offline_timer = Instant::now().duration_since(*hosts.last_connection.read().await);

            if hosts.channels().await.is_empty() && offline_timer >= offline_limit {
                warn!(target: "net::refinery", "No connections for {}s. GreylistRefinery paused.",
                          offline_timer.as_secs());

                // It is neccessary to clear suspended hosts at this point, otherwise these
                // hosts cannot be connected to in Outbound Session. Failure to do this could
                // result in the refinery being paused forver (since connections could never be
                // made).
                let suspended_hosts = hosts.suspended().await;
                for host in suspended_hosts {
                    hosts.unregister(&host).await;
                }

                continue
            }

            // Only attempt to refine peers that match our transports.
            match hosts
                .container
                .fetch_random_with_schemes(HostColor::Grey, &settings.allowed_transports)
                .await
            {
                Some((entry, position)) => {
                    let url = &entry.0;

                    if let Err(e) = hosts.try_register(url.clone(), HostState::Refine).await {
                        debug!(target: "net::refinery", "Unable to refine addr={}, err={}",
                               url.clone(), e);
                        continue
                    }

                    // Freeze the greylist in this state. Necessary since the greylist
                    // can be modified by `hosts::move_host()` or `hosts::store()`.
                    let mut greylist =
                        hosts.container.hostlists[HostColor::Grey as usize].write().await;

                    if !self.session().handshake_node(url.clone(), self.p2p().clone()).await {
                        greylist.remove(position);

                        debug!(
                            target: "net::refinery",
                            "Peer {} is non-responsive. Removed from greylist", url,
                        );

                        // Remove this entry from HostRegistry to avoid this host getting
                        // stuck in the Refining state. This is a safe since the hostlist
                        // modification is now complete.
                        hosts.unregister(url).await;

                        drop(greylist);

                        continue
                    }

                    drop(greylist);

                    debug!(
                        target: "net::refinery",
                        "Peer {} is responsive. Adding to whitelist", url,
                    );
                    let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();

                    // Add to the whitelist and remove from the greylist.
                    hosts.move_host(url, last_seen, HostColor::White).await.unwrap();

                    // When move is complete we can safely stop tracking this peer.
                    hosts.unregister(url).await;

                    debug!(target: "net::refinery", "GreylistRefinery complete!");
                    continue
                }
                None => {
                    debug!(target: "net::refinery", "No matching greylist entries found. Cannot proceed with refinery");

                    continue
                }
            }
        }
    }

    fn session(&self) -> RefineSessionPtr {
        self.session.upgrade()
    }

    fn p2p(&self) -> P2pPtr {
        self.session().p2p()
    }
}

/// Periodically try to do a version exchange with our own external
/// addresses. If the version exchange is successful, take a timestamp and
/// save it along with the external addresses. Each address along with its
/// timestamp (the `last_seen` data field) is sent in to other nodes in
/// ProtocolAddr and ProtocolSeed.
///
/// On first run, SelfHandshake will immediately conduct a version exchange
/// with our external addresses, and if successful update the last_seen
/// field. The process will wait [TODO: self_handshake_interval) before retrying.
///
/// There are two situations in which this can fail:
///
///     1. If our external address is misconfigured
///     2. If we have reached our inbound connection limit.
///
/// If our external address is misconfigured, doing a version exchange
/// with ourselves will not work and so the external addresses will not
/// be shared with other nodes.
///
/// If we have reached our inbound connection limit, the external address
/// will continue to be broadcast with an older `last_seen` (from before
/// our inbound connection was reached).
pub struct SelfHandshake {
    process: StoppableTaskPtr,
    session: LazyWeak<RefineSession>,
    pub(in crate::net) addrs: Mutex<HashMap<Url, u64>>,
}

impl SelfHandshake {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            process: StoppableTask::new(),
            session: LazyWeak::new(),
            addrs: Mutex::new(HashMap::new()),
        })
    }

    async fn start(self: Arc<Self>) {
        let ex = self.session().p2p().executor();
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
        let external_addrs = self.session().p2p().settings().external_addrs.clone();
        let mut current_attempt = 0;

        loop {
            if current_attempt >= 1 {
                // TODO: make this a configurable interval
                sleep(600).await;
            }

            // Only proceed if the external address is configured.
            if external_addrs.is_empty() {
                current_attempt += 1;
                continue
            }

            for addr in external_addrs.iter() {
                debug!(target: "net::refine_session::self_handshake",
                "Attempting a version exchange addr={}", addr);

                if self.session().handshake_node(addr.clone(), self.session().p2p()).await {
                    debug!(target: "net::refine_session::self_handshake",
                    "Version exchange successful! Updating last seen addr={}", addr);
                    let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();
                    let mut addrs = self.addrs.lock().await;

                    if addrs.contains_key(addr) {
                        let val = addrs.get_mut(addr).unwrap();
                        *val = last_seen;
                    }
                    addrs.insert(addr.clone(), last_seen);
                } else {
                    // Either our external addr is invalid or our max inbound
                    // connection count has been reached.
                    warn!(target: "net::refine_session::self_handshake",
                    "Version exchange failed! addr={}", addr);
                }
            }
            current_attempt += 1;
        }
    }

    fn session(&self) -> RefineSessionPtr {
        self.session.upgrade()
    }
}
