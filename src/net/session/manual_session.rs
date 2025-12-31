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

//! Manual connections session. Manages the creation of manual sessions.
//! Used to create a manual session and to stop and start the session.
//!
//! A manual session is a type of outbound session in which we attempt
//! connection to a predefined set of peers. Manual sessions loop forever
//! continually trying to connect to a given peer, and sleep
//! `outbound_connect_timeout` times between each attempt.
//!
//! Class consists of a weak pointer to the p2p interface and a vector of
//! outbound connection slots. Using a weak pointer to p2p allows us to
//! avoid circular dependencies. The vector of slots is wrapped in a mutex
//! lock. This is switched on every time we instantiate a connection slot
//! and insures that no other part of the program uses the slots at the
//! same time.

use std::sync::{Arc, Weak};

use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use smol::lock::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tracing::{debug, error, warn};
use url::Url;

use super::{
    super::{
        connector::Connector,
        p2p::{P2p, P2pPtr},
    },
    Session, SessionBitFlag, SESSION_MANUAL,
};
use crate::{
    net::{hosts::HostState, settings::Settings},
    system::{sleep, StoppableTask, StoppableTaskPtr},
    util::logger::verbose,
    Error, Result,
};

pub type ManualSessionPtr = Arc<ManualSession>;

/// Defines manual connections session.
pub struct ManualSession {
    pub(in crate::net) p2p: Weak<P2p>,
    slots: AsyncMutex<Vec<Arc<Slot>>>,
}

impl ManualSession {
    /// Create a new manual session.
    pub fn new(p2p: Weak<P2p>) -> ManualSessionPtr {
        Arc::new(Self { p2p, slots: AsyncMutex::new(Vec::new()) })
    }

    pub(crate) async fn start(self: Arc<Self>) {
        // Activate mutex lock on connection slots.
        let mut slots = self.slots.lock().await;

        let mut futures = FuturesUnordered::new();

        let self_ = Arc::downgrade(&self);

        // Initialize a slot for each configured peer.
        // Connections will be started by not yet activated.
        for peer in &self.p2p().settings().read().await.peers {
            let slot = Slot::new(self_.clone(), peer.clone(), self.p2p().settings());
            futures.push(slot.clone().start());
            slots.push(slot);
        }

        while (futures.next().await).is_some() {}
    }

    /// Stops the manual session.
    pub async fn stop(&self) {
        let slots = &*self.slots.lock().await;
        let mut futures = FuturesUnordered::new();

        for slot in slots {
            futures.push(slot.stop());
        }

        while (futures.next().await).is_some() {}
    }
}

#[async_trait]
impl Session for ManualSession {
    fn p2p(&self) -> P2pPtr {
        self.p2p.upgrade().unwrap()
    }

    fn type_id(&self) -> SessionBitFlag {
        SESSION_MANUAL
    }

    async fn reload(self: Arc<Self>) {}
}

struct Slot {
    addr: Url,
    process: StoppableTaskPtr,
    session: Weak<ManualSession>,
    connector: Connector,
}

impl Slot {
    fn new(
        session: Weak<ManualSession>,
        addr: Url,
        settings: Arc<AsyncRwLock<Settings>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            addr,
            process: StoppableTask::new(),
            session: session.clone(),
            connector: Connector::new(settings, session),
        })
    }

    async fn start(self: Arc<Self>) {
        let ex = self.p2p().executor();

        self.process.clone().start(
            self.run(),
            |res| async {
                match res {
                    Ok(()) | Err(Error::NetworkServiceStopped) => {}
                    Err(e) => error!("net::manual_session {e}"),
                }
            },
            Error::NetworkServiceStopped,
            ex,
        );
    }

    /// Attempts a connection on the associated Connector object.
    async fn run(self: Arc<Self>) -> Result<()> {
        let ex = self.p2p().executor();

        let mut attempts = 0;
        loop {
            attempts += 1;

            verbose!(
                target: "net::manual_session",
                "[P2P] Connecting to manual outbound [{}] (attempt #{})",
                self.addr, attempts
            );

            let settings = self.p2p().settings().read_arc().await;
            let seeds = settings.seeds.clone();
            let outbound_connect_timeout = settings.outbound_connect_timeout(self.addr.scheme());
            drop(settings);

            // Do not establish a connection to a host that is also configured as a seed.
            // This indicates a user misconfiguration.
            if seeds.contains(&self.addr) {
                error!(
                    target: "net::manual_session",
                    "[P2P] Suspending manual connection to seed [{}]", self.addr.clone(),
                );
                return Ok(())
            }

            if let Err(e) = self.p2p().hosts().try_register(self.addr.clone(), HostState::Connect) {
                debug!(target: "net::manual_session",
                    "Cannot connect to manual={}, err={e}", &self.addr);

                sleep(outbound_connect_timeout).await;

                continue
            }

            match self.connector.connect(&self.addr).await {
                Ok((_, channel)) => {
                    verbose!(
                        target: "net::manual_session",
                        "[P2P] Manual outbound connected [{}]",
                        channel.display_address()
                    );

                    let stop_sub = channel.subscribe_stop().await?;

                    // Channel is now connected but not yet setup

                    // Register the new channel
                    match self.session().register_channel(channel.clone(), ex.clone()).await {
                        Ok(()) => {
                            // Wait for channel to close
                            stop_sub.receive().await;

                            verbose!(
                                target: "net::manual_session",
                                "[P2P] Manual outbound disconnected [{}]",
                                channel.display_address()
                            );
                        }
                        Err(e) => {
                            warn!(
                                target: "net::manual_session",
                                "[P2P] Unable to connect to manual outbound [{}]: {e}",
                                channel.display_address(),
                            );

                            // Free up this addr for future operations.
                            if let Err(e) = self.p2p().hosts().unregister(channel.address()) {
                                warn!(target: "net::manual_session", "[P2P] Error while unregistering addr={}, err={e}", channel.display_address());
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        target: "net::manual_session",
                        "[P2P] Unable to connect to manual outbound: {e}",
                    );

                    // Free up this addr for future operations.
                    if let Err(e) = self.p2p().hosts().unregister(&self.addr) {
                        warn!(target: "net::manual_session", "[P2P] Error while unregistering addr={}, err={e}", self.addr);
                    }
                }
            }

            verbose!(
                target: "net::manual_session",
                "[P2P] Waiting {outbound_connect_timeout} seconds until next manual outbound connection attempt [{}]",
                self.addr,
            );

            sleep(outbound_connect_timeout).await;
        }
    }

    fn session(&self) -> ManualSessionPtr {
        self.session.upgrade().unwrap()
    }

    fn p2p(&self) -> P2pPtr {
        self.session().p2p()
    }

    async fn stop(&self) {
        self.connector.stop();
        self.process.stop().await;
    }
}
