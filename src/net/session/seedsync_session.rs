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

//! Seed sync session creates a connection to the seed nodes specified in settings.
//! A new seed sync session is created every time we call [`P2p::start()`]. The
//! seed sync session loops through all the configured seeds and tries to connect
//! to them using a [`Connector`]. Seed sync either connects successfully, fails
//! with an error, or times out.
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
    atomic::{AtomicUsize, Ordering},
    Arc, Weak,
};

use async_trait::async_trait;
use futures::future::join_all;
use log::{debug, info, warn};
use smol::Executor;
use url::Url;

use super::{
    super::{
        connector::Connector,
        p2p::{P2p, P2pPtr},
    },
    Session, SessionBitFlag, SESSION_SEED,
};
use crate::{Error, Result};

pub type SeedSyncSessionPtr = Arc<SeedSyncSession>;

/// Defines seed connections session
pub struct SeedSyncSession {
    p2p: Weak<P2p>,
}

impl SeedSyncSession {
    /// Create a new seed sync session instance
    pub fn new(p2p: Weak<P2p>) -> SeedSyncSessionPtr {
        Arc::new(Self { p2p })
    }

    /// Start the seed sync session. Creates a new task for every seed
    /// connection and starts the seed on each task.
    pub async fn start(self: Arc<Self>) -> Result<()> {
        debug!(target: "net::session::seedsync_session", "SeedSyncSession::start() [START]");
        let settings = self.p2p().settings();

        if settings.seeds.is_empty() {
            warn!(
                target: "net::session::seedsync_session",
                "[P2P] Skipping seed sync process since no seeds are configured.",
            );

            return Ok(())
        }

        // Gather tasks so we can execute concurrently
        let executor = self.p2p().executor();
        let mut tasks = Vec::with_capacity(settings.seeds.len());

        let failed = Arc::new(AtomicUsize::new(0));

        for (i, seed) in settings.seeds.iter().enumerate() {
            let ex_ = executor.clone();
            let self_ = self.clone();
            let failed_ = failed.clone();

            tasks.push(async move {
                if let Err(e) = self_.clone().start_seed(i, seed.clone(), ex_.clone()).await {
                    warn!(
                        target: "net::session::seedsync_session",
                        "[P2P] Seed #{} connection failed: {}", i, e,
                    );
                    failed_.fetch_add(1, Ordering::SeqCst);
                }
            });
        }

        // Poll concurrently
        join_all(tasks).await;

        if failed.load(Ordering::SeqCst) == settings.seeds.len() {
            return Err(Error::SeedFailed)
        }

        // Seed process complete
        if self.p2p().hosts().is_empty_greylist().await {
            warn!(target: "net::session::seedsync_session", "[P2P] Hosts pool empty after seeding");
        }

        debug!(target: "net::session::seedsync_session", "SeedSyncSession::start() [END]");
        Ok(())
    }

    /// Connects to a seed socket address
    async fn start_seed(
        self: Arc<Self>,
        seed_index: usize,
        seed: Url,
        ex: Arc<Executor<'_>>,
    ) -> Result<()> {
        debug!(
            target: "net::session::seedsync_session", "SeedSyncSession::start_seed(i={}) [START]",
            seed_index
        );

        let settings = self.p2p.upgrade().unwrap().settings();
        let parent = Arc::downgrade(&self);
        let connector = Connector::new(settings.clone(), parent);

        match connector.connect(&seed).await {
            Ok((url, ch)) => {
                info!(
                    target: "net::session::seedsync_session",
                    "[P2P] Connected seed #{} [{}]", seed_index, url,
                );

                if let Err(e) = self.clone().register_channel(ch.clone(), ex.clone()).await {
                    warn!(
                        target: "net::session::seedsync_session",
                        "[P2P] Failure during sync seed session #{} [{}]: {}",
                        seed_index, url, e,
                    );
                }

                info!(
                    target: "net::session::seedsync_session",
                    "[P2P] Disconnecting from seed #{} [{}]",
                    seed_index, url,
                );
                ch.stop().await;
            }

            Err(e) => {
                warn!(
                    target: "net::session:seedsync_session",
                    "[P2P] Failure contacting seed #{} [{}]: {}",
                    seed_index, seed, e
                );
                return Err(e)
            }
        }

        debug!(
            target: "net::session::seedsync_session",
            "SeedSyncSession::start_seed(i={}) [END]",
            seed_index
        );

        Ok(())
    }
}

#[async_trait]
impl Session for SeedSyncSession {
    fn p2p(&self) -> P2pPtr {
        self.p2p.upgrade().unwrap()
    }

    fn type_id(&self) -> SessionBitFlag {
        SESSION_SEED
    }
}
