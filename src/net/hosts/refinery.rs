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

use std::{
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

use log::{debug, warn};
use url::Url;

use super::super::p2p::{P2p, P2pPtr};
use crate::{
    net::{connector::Connector, protocol::ProtocolVersion, session::Session},
    system::{
        run_until_completion, sleep, timeout::timeout, LazyWeak, StoppableTask, StoppableTaskPtr,
    },
    Error,
};

pub type GreylistRefineryPtr = Arc<GreylistRefinery>;

/// Probe random peers on the greylist. If a peer is responsive, update the last_seen field and
/// add it to the whitelist. If a node does not respond, remove it from the greylist.
/// Called periodically.
pub struct GreylistRefinery {
    /// Weak pointer to parent p2p object
    pub(in crate::net) p2p: LazyWeak<P2p>,
    process: StoppableTaskPtr,
}

impl GreylistRefinery {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { p2p: LazyWeak::new(), process: StoppableTask::new() })
    }

    pub async fn start(self: Arc<Self>) {
        match self.p2p().hosts().load_hosts().await {
            Ok(()) => {
                debug!(target: "net::refinery::start()", "Load hosts successful!");
            }
            Err(e) => {
                warn!(target: "net::refinery::start()", "Error loading hosts {}", e);
            }
        }
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

        match self.p2p().hosts().save_hosts().await {
            Ok(()) => {
                debug!(target: "net::refinery::stop()", "Save hosts successful!");
            }
            Err(e) => {
                warn!(target: "net::refinery::stop()", "Error saving hosts {}", e);
            }
        }
    }

    // Randomly select a peer on the greylist and probe it.
    async fn run(self: Arc<Self>) {
        loop {
            sleep(self.p2p().settings().greylist_refinery_interval).await;

            let hosts = self.p2p().hosts();

            if hosts.is_empty_greylist().await {
                debug!(target: "net::refinery",
                "Greylist is empty! Cannot start refinery process");

                continue
            }

            // Only attempt to refine peers that match our transports.
            match hosts.greylist_fetch_random_with_schemes().await {
                Some((entry, position)) => {
                    let url = &entry.0;

                    // Skip this node if it's being migrated currently.
                    if hosts.is_migrating(url).await {
                        continue
                    }

                    // Don't refine nodes that we are already connected to.
                    if self.p2p().exists(url).await {
                        continue
                    }

                    // Don't refine nodes that we are trying to connect to.
                    if !self.p2p().add_pending(url).await {
                        continue
                    }

                    let mut greylist = hosts.greylist.write().await;
                    if !ping_node(url.clone(), self.p2p().clone()).await {
                        greylist.remove(position);

                        // Remove connection from pending
                        self.p2p().remove_pending(url).await;
                        debug!(
                            target: "net::refinery",
                            "Peer {} is non-responsive. Removed from greylist", url,
                        );

                        continue
                    }
                    drop(greylist);

                    let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();

                    // Append to the whitelist.
                    hosts.whitelist_store_or_update(&[(url.clone(), last_seen)]).await;

                    // Remove whitelisted peer from the greylist.
                    hosts.greylist_remove(url, position).await;

                    // Remove connection from pending
                    self.p2p().remove_pending(url).await;
                }
                None => {
                    debug!(target: "net::refinery", "No matching greylist entries found. Cannot proceed with refinery");

                    continue
                }
            }
        }
    }

    fn p2p(&self) -> P2pPtr {
        self.p2p.upgrade()
    }
}

/// Check a node is online by establishing a channel with it and conducting a handshake with a
/// version exchange.
///
/// We must use run_until_completion() to ensure this code will complete even if the parent task
/// has been destroyed. Otherwise ping_node() will become a zombie process if the rest of the p2p
/// network has been shutdown but the handshake it still ongoing.
///
/// Other parts of the p2p stack have safe shutdown methods built into them due to the ownership
/// structure. Here we are creating a outbound session that is not owned by anything and is not
/// so is not safely cancelled on shutdown.
pub async fn ping_node(addr: Url, p2p: P2pPtr) -> bool {
    let ex = p2p.executor();
    run_until_completion(ping_node_impl(addr.clone(), p2p), ex).await
}

async fn ping_node_impl(addr: Url, p2p: P2pPtr) -> bool {
    let session_outbound = p2p.session_outbound();
    let parent = Arc::downgrade(&session_outbound);
    let connector = Connector::new(p2p.settings(), parent);

    debug!(target: "net::refinery::ping_node()", "Attempting to connect to {}", addr);
    match connector.connect(&addr).await {
        Ok((url, channel)) => {
            debug!(target: "net::refinery::ping_node()", "Successfully created a channel with {}", url);
            // First initialize the version protocol and its Version, Verack subscribers.
            let proto_ver = ProtocolVersion::new(channel.clone(), p2p.settings()).await;

            debug!(target: "net::refinery::ping_node()", "Performing handshake protocols with {}", url);
            // Then run the version exchange, store the channel and subscribe to a stop signal.
            let handshake_task = session_outbound.perform_handshake_protocols(
                proto_ver,
                channel.clone(),
                p2p.executor(),
            );

            debug!(target: "net::refinery::ping_node()", "Starting channel {}", url);
            channel.clone().start(p2p.executor());

            // Ensure the channel gets stopped by adding a timeout to the handshake. Otherwise if
            // the handshake does not finish channel.stop() will never get called, resulting in
            // zombie processes.
            let result = timeout(Duration::from_secs(5), handshake_task).await;

            debug!(target: "net::refinery::ping_node()", "Stopping channel {}", url);
            channel.stop().await;

            match result {
                Ok(_) => {
                    debug!(target: "net::refinery::ping_node()", "Handshake success!");
                    true
                }
                Err(e) => {
                    debug!(target: "net::refinery::ping_node()", "Handshake err: {}", e);
                    false
                }
            }
        }

        Err(e) => {
            debug!(target: "net::refinery::ping_node()", "Failed to connect to {}, ({})", addr, e);
            false
        }
    }
}
