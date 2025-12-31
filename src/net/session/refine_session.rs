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

//! `RefineSession` manages the `GreylistRefinery`, which randomly selects
//! entries on the greylist and updates them to whitelist if active,
//!
//! `GreylistRefinery` makes use of a `RefineSession` method called
//! `handshake_node()`, which uses a `Connector` to establish a `Channel` with
//! a provided address, and then does a version exchange across the channel
//! (`perform_handshake_protocols`). `handshake_node()` can either succeed,
//! fail, or timeout.

use futures::{
    future::{select, Either},
    pin_mut,
};
use smol::Timer;
use std::{
    sync::{Arc, Weak},
    time::{Duration, Instant, UNIX_EPOCH},
};

use async_trait::async_trait;
use tracing::{debug, warn};
use url::Url;

use super::super::p2p::{P2p, P2pPtr};

use crate::{
    net::{
        connector::Connector,
        hosts::{HostColor, HostState},
        protocol::ProtocolVersion,
        session::{Session, SessionBitFlag, SESSION_REFINE},
    },
    system::{sleep, StoppableTask, StoppableTaskPtr},
    Error,
};

pub type RefineSessionPtr = Arc<RefineSession>;

pub struct RefineSession {
    /// Weak pointer to parent p2p object
    pub(in crate::net) p2p: Weak<P2p>,

    /// Task that periodically checks entries in the greylist.
    pub(in crate::net) refinery: Arc<GreylistRefinery>,
}

impl RefineSession {
    pub fn new(p2p: Weak<P2p>) -> RefineSessionPtr {
        Arc::new_cyclic(|session| Self { p2p, refinery: GreylistRefinery::new(session.clone()) })
    }

    /// Start the refinery and self handshake processes.
    pub(crate) async fn start(self: Arc<Self>) {
        if let Some(ref hostlist) = self.p2p().settings().read().await.hostlist {
            match self.p2p().hosts().container.load_all(hostlist) {
                Ok(()) => {
                    debug!(target: "net::refine_session::start", "Load hosts successful!");
                }
                Err(e) => {
                    warn!(target: "net::refine_session::start", "Error loading hosts {e}");
                }
            }
        }

        match self.p2p().hosts().import_blacklist().await {
            Ok(()) => {
                debug!(target: "net::refine_session::start", "Import blacklist successful!");
            }
            Err(e) => {
                warn!(target: "net::refine_session::start",
                    "Error importing blacklist from config file {e}");
            }
        }

        debug!(target: "net::refine_session", "Starting greylist refinery process");
        self.refinery.clone().start().await;
    }

    /// Stop the refinery and self handshake processes.
    pub(crate) async fn stop(&self) {
        debug!(target: "net::refine_session", "Stopping refinery process");
        self.refinery.clone().stop().await;

        if let Some(ref hostlist) = self.p2p().settings().read().await.hostlist {
            match self.p2p().hosts().container.save_all(hostlist) {
                Ok(()) => {
                    debug!(target: "net::refine_session::stop", "Save hosts successful!");
                }
                Err(e) => {
                    warn!(target: "net::refine_session::stop", "Error saving hosts {e}");
                }
            }
        }
    }

    /// Globally accessible function to perform a version exchange with a
    /// given address.  Returns `true` if an address is accessible, false
    /// otherwise.  
    pub async fn handshake_node(self: Arc<Self>, addr: Url, p2p: P2pPtr) -> bool {
        let self_ = Arc::downgrade(&self);
        let connector = Connector::new(self.p2p().settings(), self_);

        debug!(target: "net::refinery::handshake_node", "Attempting to connect to {addr}");
        match connector.connect(&addr).await {
            Ok((url, channel)) => {
                debug!(target: "net::refinery::handshake_node", "Successfully created a channel with {url}");
                // First initialize the version protocol and its Version, Verack subscriptions.
                let proto_ver = ProtocolVersion::new(channel.clone(), p2p.settings()).await;

                debug!(target: "net::refinery::handshake_node", "Performing handshake protocols with {url}");
                // Then run the version exchange, store the channel and subscribe to a stop signal.
                let handshake =
                    self.perform_handshake_protocols(proto_ver, channel.clone(), p2p.executor());

                debug!(target: "net::refinery::handshake_node", "Starting channel {url}");
                channel.clone().start(p2p.executor());

                // Ensure the channel gets stopped by adding a timeout to the handshake. Otherwise if
                // the handshake does not finish channel.stop() will never get called, resulting in
                // zombie processes.
                let timeout = Timer::after(Duration::from_secs(5));

                pin_mut!(timeout);
                pin_mut!(handshake);

                let result = match select(handshake, timeout).await {
                    Either::Left((Ok(_), _)) => {
                        debug!(target: "net::refinery::handshake_node", "Handshake success!");
                        true
                    }
                    Either::Left((Err(e), _)) => {
                        debug!(target: "net::refinery::handshake_node", "Handshake error={e}");
                        false
                    }
                    Either::Right((_, _)) => {
                        debug!(target: "net::refinery::handshake_node", "Handshake timed out");
                        false
                    }
                };

                debug!(target: "net::refinery::handshake_node", "Stopping channel {url}");
                channel.stop().await;

                result
            }

            Err(e) => {
                debug!(target: "net::refinery::handshake_node", "Failed to connect ({e})");
                false
            }
        }
    }
}

#[async_trait]
impl Session for RefineSession {
    fn p2p(&self) -> P2pPtr {
        self.p2p.upgrade().unwrap()
    }

    fn type_id(&self) -> SessionBitFlag {
        SESSION_REFINE
    }

    async fn reload(self: Arc<Self>) {}
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
    session: Weak<RefineSession>,
    process: StoppableTaskPtr,
}

impl GreylistRefinery {
    pub fn new(session: Weak<RefineSession>) -> Arc<Self> {
        Arc::new(Self { session, process: StoppableTask::new() })
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
        self.process.stop().await;
    }

    // Randomly select a peer on the greylist and probe it. This method will remove from the
    // greylist and store on the whitelist providing the peer is responsive.
    async fn run(self: Arc<Self>) {
        let hosts = self.p2p().hosts();

        loop {
            // Acquire read lock on P2P settings and load necessary settings
            let settings = self.p2p().settings().read_arc().await;
            let greylist_refinery_interval = settings.greylist_refinery_interval;
            let time_with_no_connections = settings.time_with_no_connections;
            let active_profiles = settings.active_profiles.clone();
            drop(settings);

            sleep(greylist_refinery_interval).await;

            if hosts.container.is_empty(HostColor::Grey) {
                debug!(target: "net::refinery",
                "Greylist is empty! Cannot start refinery process");

                continue
            }

            // Pause the refinery if we've had zero connections for longer than the configured
            // limit.
            let offline_limit = Duration::from_secs(time_with_no_connections);

            let offline_timer =
                { Instant::now().duration_since(*hosts.last_connection.lock().unwrap()) };

            if !self.p2p().is_connected() && offline_timer >= offline_limit {
                warn!(target: "net::refinery", "No connections for {}s. GreylistRefinery paused.",
                          offline_timer.as_secs());

                // It is necessary to Free suspended hosts at this point, otherwise these
                // hosts cannot be connected to in Outbound Session. Failure to do this could
                // result in the refinery being paused forver (since connections could never be
                // made).
                let suspended_hosts = hosts.suspended();
                for host in suspended_hosts {
                    if let Err(e) = hosts.unregister(&host) {
                        warn!(target: "net::refinery", "Error while unregistering addr={host}, err={e}");
                    }
                }

                continue
            }

            // Only attempt to refine peers that match our transports.
            match hosts.container.fetch_random_with_schemes(HostColor::Grey, &active_profiles) {
                Some((entry, _)) => {
                    let url = &entry.0;

                    if let Err(e) = hosts.try_register(url.clone(), HostState::Refine) {
                        debug!(target: "net::refinery", "Unable to refine addr={}, err={e}",
                               url.clone());
                        continue
                    }

                    if !self.session().handshake_node(url.clone(), self.p2p().clone()).await {
                        hosts.container.remove_if_exists(HostColor::Grey, url);

                        debug!(
                            target: "net::refinery",
                            "Peer {url} handshake failed. Removed from greylist"
                        );

                        // Free up this addr for future operations.
                        if let Err(e) = hosts.unregister(url) {
                            warn!(target: "net::refinery", "Error while unregistering addr={url}, err={e}");
                        }

                        continue
                    }
                    debug!(
                        target: "net::refinery",
                        "Peer {url} handshake successful. Adding to whitelist"
                    );
                    let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();

                    hosts.whitelist_host(url, last_seen).await.unwrap();

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
        self.session.upgrade().unwrap()
    }

    fn p2p(&self) -> P2pPtr {
        self.session().p2p()
    }
}
