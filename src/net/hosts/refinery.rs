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
    time::{Duration, Instant, UNIX_EPOCH},
};

use log::{debug, warn};
use url::Url;

use super::{
    super::p2p::{P2p, P2pPtr},
    store::HostColor,
};
use crate::{
    net::{
        connector::Connector, hosts::store::HostState, protocol::ProtocolVersion, session::Session,
    },
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
        match self.p2p().hosts().container.load_all(&self.p2p().settings().hostlist).await {
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

        match self.p2p().hosts().container.save_all(&self.p2p().settings().hostlist).await {
            Ok(()) => {
                debug!(target: "net::refinery::stop()", "Save hosts successful!");
            }
            Err(e) => {
                warn!(target: "net::refinery::stop()", "Error saving hosts {}", e);
            }
        }
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
                warn!(target: "net::refinery", "No connections for {}s. Refinery paused.",
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

                    if hosts.try_register(url.clone(), HostState::Refine).await.is_err() {
                        continue
                    }

                    if !ping_node(url.clone(), self.p2p().clone()).await {
                        hosts.container.remove(HostColor::Grey, url, position).await;

                        debug!(
                            target: "net::refinery",
                            "Peer {} is non-responsive. Removed from greylist", url,
                        );

                        // Remove this entry from HostRegistry to avoid this host getting
                        // stuck in the Refining state.
                        //
                        // It is not necessary to call this when the refinery passes, since the
                        // state will be changed to Connected.
                        hosts.unregister(url).await;

                        continue
                    }

                    let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();

                    // Add to the whitelist and remove from the greylist.
                    hosts.move_host(url, last_seen, HostColor::White, false, None).await;
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
