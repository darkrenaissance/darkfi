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

//! Manual connections session. Manages the creation of manual sessions.
//! Used to create a manual session and to stop and start the session.
//!
//! A manual session is a type of outbound session in which we attempt
//! connection to a predefined set of peers.
//!
//! Class consists of a weak pointer to the p2p interface and a vector of
//! outbound connection slots. Using a weak pointer to p2p allows us to
//! avoid circular dependencies. The vector of slots is wrapped in a mutex
//! lock. This is switched on every time we instantiate a connection slot
//! and insures that no other part of the program uses the slots at the
//! same time.

use std::sync::Arc;

use async_trait::async_trait;
use log::{debug, error, info, warn};
use smol::lock::Mutex;
use url::Url;

use super::{
    super::{
        connector::Connector,
        p2p::{P2p, P2pPtr},
    },
    Session, SessionBitFlag, SESSION_MANUAL,
};
use crate::{
    net::hosts::HostState,
    system::{sleep, LazyWeak, StoppableTask, StoppableTaskPtr},
    Error, Result,
};

pub type ManualSessionPtr = Arc<ManualSession>;

/// Defines manual connections session.
pub struct ManualSession {
    pub(in crate::net) p2p: LazyWeak<P2p>,
    connect_slots: Mutex<Vec<StoppableTaskPtr>>,
}

impl ManualSession {
    /// Create a new manual session.
    pub fn new() -> ManualSessionPtr {
        Arc::new(Self { p2p: LazyWeak::new(), connect_slots: Mutex::new(Vec::new()) })
    }

    /// Stops the manual session.
    pub async fn stop(&self) {
        let connect_slots = &*self.connect_slots.lock().await;

        for slot in connect_slots {
            slot.stop().await;
        }
    }

    /// Connect the manual session to the given address
    pub async fn connect(self: Arc<Self>, addr: Url) {
        let ex = self.p2p().executor();
        let task = StoppableTask::new();
        self.connect_slots.lock().await.push(task.clone());

        task.start(
            self.clone().channel_connect_loop(addr),
            // Ignore stop handler
            |_| async {},
            Error::NetworkServiceStopped,
            ex,
        );
    }

    /// Creates a connector object and tries to connect using it
    pub async fn channel_connect_loop(self: Arc<Self>, addr: Url) -> Result<()> {
        let ex = self.p2p().executor();
        let parent = Arc::downgrade(&self);
        let settings = self.p2p().settings();
        let connector = Connector::new(settings.clone(), parent);

        let attempts = settings.manual_attempt_limit;
        let mut remaining = attempts;

        // Loop forever if attempts==0, otherwise loop attempts number of times.
        let mut tried_attempts = 0;
        loop {
            tried_attempts += 1;
            info!(
                target: "net::manual_session",
                "[P2P] Connecting to manual outbound [{}] (attempt #{})",
                addr, tried_attempts,
            );

            // Do not establish a connection to a host that is also configured as a seed.
            // This indicates a user misconfiguration.
            if settings.seeds.contains(&addr) {
                error!(target: "net::manual_session", 
                       "[P2P] Suspending manual connection to seed [{}]", addr.clone());
                return Ok(())
            }

            match self.p2p().hosts().try_register(addr.clone(), HostState::Connect).await {
                Ok(_) => {
                    match connector.connect(&addr).await {
                        Ok((url, channel)) => {
                            info!(
                                target: "net::manual_session",
                                "[P2P] Manual outbound connected [{}]", url,
                            );

                            let stop_sub = channel
                                .subscribe_stop()
                                .await
                                .expect("Channel should not be stopped");

                            // Channel is now connected but not yet setup

                            // Register the new channel
                            self.register_channel(channel.clone(), ex.clone()).await?;

                            // Wait for channel to close
                            stop_sub.receive().await;

                            info!(
                                target: "net::manual_session",
                                "[P2P] Manual outbound disconnected [{}]", url,
                            );

                            // DEV NOTE: Here we can choose to attempt reconnection again
                            return Ok(())
                        }
                        Err(e) => {
                            warn!(
                                target: "net::manual_session",
                                "[P2P] Unable to connect to manual outbound [{}]: {}",
                                addr, e,
                            );

                            // Stop tracking this peer, to avoid it getting stuck in the Connect
                            // state. This is safe since we have failed to connect at this point.
                            self.p2p().hosts().unregister(&addr).await;
                        }
                    }
                }
                // This address is currently unavailable.
                Err(e) => {
                    debug!(target: "net::manual_session", "[P2P] Unable to connect to manual
                           outbound [{}]: {}", addr.clone(), e);
                }
            }
            // Wait and try again.
            // TODO: Should we notify about the failure now, or after all attempts
            // have failed?
            self.p2p().hosts().channel_subscriber.notify(Err(Error::ConnectFailed)).await;

            remaining = if attempts == 0 { 1 } else { remaining - 1 };
            if remaining == 0 {
                break
            }

            info!(
                target: "net::manual_session",
                "[P2P] Waiting {} seconds until next manual outbound connection attempt [{}]",
                settings.outbound_connect_timeout, addr,
            );
            sleep(settings.outbound_connect_timeout).await;
        }

        warn!(
            target: "net::manual_session",
            "[P2P] Suspending manual connection to {} after {} failed attempts",
            addr, attempts,
        );

        Ok(())
    }
}

#[async_trait]
impl Session for ManualSession {
    fn p2p(&self) -> P2pPtr {
        self.p2p.upgrade()
    }

    fn type_id(&self) -> SessionBitFlag {
        SESSION_MANUAL
    }
}
