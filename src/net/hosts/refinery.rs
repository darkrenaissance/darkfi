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

use std::{sync::Arc, time::UNIX_EPOCH};

use log::{debug, warn};
use url::Url;

use super::super::p2p::{P2p, P2pPtr};
use crate::{
    net::{connector::Connector, protocol::ProtocolVersion, session::Session},
    system::{sleep, LazyWeak, StoppableTask, StoppableTaskPtr},
    Error,
};

pub type GreylistRefineryPtr = Arc<GreylistRefinery>;

//// Probe random peers on the greylist. If a peer is responsive, update the last_seen field and
//// add it to the whitelist. If a node does not respond, remove it from the greylist.
//// Called periodically.
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
        match self.p2p().hosts().save_hosts().await {
            Ok(()) => {
                debug!(target: "deadlock", "saved hosts node {}", self.p2p().settings().node_id);
                debug!(target: "net::refinery::stop()", "Save hosts successful!");
            }
            Err(e) => {
                debug!(target: "deadlock", "ERROR saving hosts node {}", self.p2p().settings().node_id);
                warn!(target: "net::refinery::stop()", "Error saving hosts {}", e);
            }
        }
        debug!(target: "deadlock", "Stopping refinery process node {}", self.p2p().settings().node_id);
        self.process.stop().await;
        debug!(target: "deadlock", "Refinery process stopped node {}", self.p2p().settings().node_id);
    }

    // Randomly select a peer on the greylist and probe it.
    async fn run(self: Arc<Self>) {
        //debug!(target: "net::refinery::run()", "START");
        debug!(target: "deadlock", "refinery START node {}", self.p2p().settings().node_id);
        loop {
            sleep(self.p2p().settings().greylist_refinery_interval).await;

            let hosts = self.p2p().hosts();

            if hosts.is_empty_greylist().await {
                debug!(target: "deadlock", "Greylist is empty! Cannot start refinery node {}", self.p2p().settings().node_id);
                warn!(target: "net::refinery::run()",
                "Greylist is empty! Cannot start refinery process");

                continue
            }

            let (entry, position) = hosts.greylist_fetch_random().await;
            let url = &entry.0;

            if !ping_node(url, self.p2p().clone()).await {
                let mut greylist = hosts.greylist.write().await;
                greylist.remove(position);
                //debug!(target: "net::refinery::run()", "Peer {} is not response. Removed from greylist", url);
                debug!(target: "deadlock", "Peer {} is not response. Removed from greylist node {}", url, self.p2p().settings().node_id);

                continue
            }

            let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();

            debug!(target: "deadlock", "whitelist store or update node {}", self.p2p().settings().node_id);
            // Append to the whitelist.
            hosts.whitelist_store_or_update(&[(url.clone(), last_seen)]).await.unwrap();

            debug!(target: "deadlock", "greylist remove node {}", self.p2p().settings().node_id);
            // Remove whitelisted peer from the greylist.
            hosts.greylist_remove(url, position).await;

            debug!(target: "deadlock", "refinery STOP node {}", self.p2p().settings().node_id);
        }
    }

    fn p2p(&self) -> P2pPtr {
        self.p2p.upgrade()
    }
}

// Ping a node to check it's online.
pub async fn ping_node(addr: &Url, p2p: P2pPtr) -> bool {
    debug!(target: "deadlock", "ping node START node {}", p2p.settings().node_id);
    let session_outbound = p2p.session_outbound();
    let parent = Arc::downgrade(&session_outbound);
    let connector = Connector::new(p2p.settings(), parent);

    debug!(target: "net::refinery::ping_node()", "Attempting to connect to {}", addr);
    match connector.connect(addr).await {
        Ok((_url, channel)) => {
            debug!(target: "net::refinery::ping_node()", "Connected successfully!");
            let proto_ver = ProtocolVersion::new(channel.clone(), p2p.settings()).await;

            let handshake_task = session_outbound.perform_handshake_protocols(
                proto_ver,
                channel.clone(),
                p2p.executor(),
            );

            channel.clone().start(p2p.executor());

            match handshake_task.await {
                Ok(()) => {
                    debug!(target: "net::refinery::ping_node()", "Handshake success! Stopping channel.");
                    debug!(target: "deadlock", "ping node STOP node {} -> true", p2p.settings().node_id);
                    channel.stop().await;
                    true
                }
                Err(e) => {
                    debug!(target: "net::refinery::ping_node()", "Handshake failure! {}", e);
                    debug!(target: "deadlock", "ping node STOP node {} -> false", p2p.settings().node_id);
                    channel.stop().await;
                    false
                }
            }
        }

        Err(e) => {
            debug!(target: "net::refinery::ping_node()", "Failed to connect to {}, ({})", addr, e);
            debug!(target: "deadlock", "ping node STOP node {} -> false", p2p.settings().node_id);
            false
        }
    }
}
