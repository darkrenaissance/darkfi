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

//! Seed sync session creates a connection to the seed nodes specified in settings.
//! A new seed sync session is created every time we call [`P2p::start()`]. The
//! seed sync session loops through all the configured seeds and creates a corresponding
//! `Slot`. `Slot`'s are started, but sit in a suspended state until they are activated
//! by a call to notify (see: `p2p.seed()`).
//!
//! When a `Slot` has been activated by a call to `notify()`, it will try to connect
//! to the given seed address using a [`Connector`]. This will either connect successfully
//! or fail with a warning. With gather the results of each `Slot` in an `AtomicBool`
//! so that we can handle the error elsewhere in the code base.
//!
//! If a seed node connects successfully, it runs a version exchange protocol,
//! stores the channel in the p2p list of channels, and disconnects, removing
//! the channel from the channel list.
//!
//! The channel is registered using the [`Session::register_channel()`] trait
//! method. This invokes the Protocol Registry method `attach()`. Usually this
//! returns a list of protocols that we loop through and start. In this case,
//! `attach()` uses the bitflag selector to identify seed sessions and exclude
//! them.
//!
//! The version exchange occurs inside `register_channel()`. We create a handshake
//! task that runs the version exchange with the `perform_handshake_protocols()`
//! function. This runs the version exchange protocol, stores the channel in the
//! p2p list of channels, and subscribes to a stop signal.

use std::sync::{
    atomic::{AtomicBool, Ordering::SeqCst},
    Arc, Weak,
};

use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use log::{debug, info, warn};
use smol::lock::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use url::Url;

use super::{
    super::{
        connector::Connector,
        hosts::HostColor,
        p2p::{P2p, P2pPtr},
        settings::Settings,
    },
    Session, SessionBitFlag, SESSION_SEED,
};
use crate::{
    net::hosts::HostState,
    system::{CondVar, LazyWeak, StoppableTask, StoppableTaskPtr},
    Error,
};

pub type SeedSyncSessionPtr = Arc<SeedSyncSession>;

/// Defines seed connections session
pub struct SeedSyncSession {
    pub(in crate::net) p2p: LazyWeak<P2p>,
    slots: AsyncMutex<Vec<Arc<Slot>>>,
}

impl SeedSyncSession {
    /// Create a new seed sync session instance
    pub(crate) fn new() -> SeedSyncSessionPtr {
        Arc::new(Self { p2p: LazyWeak::new(), slots: AsyncMutex::new(Vec::new()) })
    }

    /// Initialize the seedsync session. Each slot is suspended while it waits
    /// for a call to notify().
    pub(crate) async fn start(self: Arc<Self>) {
        // Activate mutex lock on connection slots.
        let mut slots = self.slots.lock().await;

        let mut futures = FuturesUnordered::new();

        let self_ = Arc::downgrade(&self);

        // Initialize a slot for each configured seed.
        // Connections will be started by not yet activated.
        for seed in &self.p2p().settings().read().await.seeds {
            let slot = Slot::new(self_.clone(), seed.clone(), self.p2p().settings());
            futures.push(slot.clone().start());
            slots.push(slot);
        }

        while (futures.next().await).is_some() {}
    }

    /// Activate the slots so they can continue with the seedsync process.
    /// Called in `p2p.seed()`.
    pub(crate) async fn notify(&self) {
        let slots = &*self.slots.lock().await;

        for slot in slots {
            slot.notify();
        }
    }

    /// Stop the seedsync session.
    pub(crate) async fn stop(&self) {
        debug!(target: "net::seedsync_session", "Stopping seed sync session...");
        let slots = &*self.slots.lock().await;
        let mut futures = FuturesUnordered::new();

        for slot in slots {
            futures.push(slot.clone().stop());
        }

        while (futures.next().await).is_some() {}
        debug!(target: "net::seedsync_session", "Seed sync session stopped!");
    }

    pub(crate) async fn failed(&self) -> bool {
        let slots = &*self.slots.lock().await;
        slots.iter().any(|s| s.failed())
    }
}

#[async_trait]
impl Session for SeedSyncSession {
    fn p2p(&self) -> P2pPtr {
        self.p2p.upgrade()
    }

    fn type_id(&self) -> SessionBitFlag {
        SESSION_SEED
    }
}

struct Slot {
    addr: Url,
    process: StoppableTaskPtr,
    wakeup_self: CondVar,
    session: Weak<SeedSyncSession>,
    connector: Connector,
    failed: AtomicBool,
}

impl Slot {
    fn new(
        session: Weak<SeedSyncSession>,
        addr: Url,
        settings: Arc<AsyncRwLock<Settings>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            addr,
            process: StoppableTask::new(),
            wakeup_self: CondVar::new(),
            session: session.clone(),
            connector: Connector::new(settings, session),
            failed: AtomicBool::new(false),
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

    /// Main seedsync connection process that is started on `p2p.start()` but does
    /// not proceed until it receives a call to `notify()` (called in `p2p.seed()`).
    /// Resets the CondVar after each run to re-suspend the connection process until
    /// `notify()` is called again.
    async fn run(self: Arc<Self>) {
        let ex = self.p2p().executor();
        let hosts = self.p2p().hosts();

        loop {
            // Wait for a signal from notify() before proceeding with the seedsync.
            self.wait().await;

            debug!(
                target: "net::session::seedsync_session", "SeedSyncSession::start_seed() [START]",
            );

            if let Err(e) = hosts.try_register(self.addr.clone(), HostState::Connect) {
                debug!(target: "net::session::seedsync_session",
                    "Cannot connect to seed={}, err={}", &self.addr, e);

                // Reset the CondVar for future use.
                self.reset();

                continue
            }

            match self.connector.connect(&self.addr).await {
                Ok((url, ch)) => {
                    info!(
                        target: "net::session::seedsync_session",
                        "[P2P] Connected seed [{}]", url,
                    );

                    match self.session().register_channel(ch.clone(), ex.clone()).await {
                        Ok(()) => {
                            self.failed.store(false, SeqCst);
                        }

                        Err(e) => {
                            warn!(
                                target: "net::session::seedsync_session",
                                "[P2P] Failure during sync seed session [{}]: {}",
                                url, e,
                            );
                            self.failed.store(true, SeqCst);
                        }
                    }

                    info!(
                        target: "net::session::seedsync_session",
                        "[P2P] Disconnecting from seed [{}]",
                        url,
                    );
                    ch.stop().await;
                }

                Err(e) => {
                    warn!(
                        target: "net::session:seedsync_session",
                        "[P2P] Failure contacting seed [{}]: {}",
                        self.addr, e
                    );

                    self.failed.store(true, SeqCst);

                    // Reset the CondVar for future use.
                    self.reset();

                    continue
                }
            }

            // Seed process complete
            if hosts.container.is_empty(HostColor::Grey) {
                warn!(target: "net::session::seedsync_session()",
                "[P2P] Greylist empty after seeding");
            }

            // Reset the CondVar for future use.
            self.reset();

            debug!(
                target: "net::session::seedsync_session",
                "SeedSyncSession::start_seed() [END]",
            );
        }
    }

    pub fn failed(&self) -> bool {
        self.failed.load(SeqCst)
    }

    fn session(&self) -> SeedSyncSessionPtr {
        self.session.upgrade().unwrap()
    }

    fn p2p(&self) -> P2pPtr {
        self.session().p2p()
    }

    async fn wait(&self) {
        self.wakeup_self.wait().await;
    }

    fn reset(&self) {
        self.wakeup_self.reset()
    }

    fn notify(&self) {
        self.wakeup_self.notify()
    }

    async fn stop(self: Arc<Self>) {
        self.connector.stop();
        self.process.stop().await;
    }
}
