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

use std::{
    collections::HashSet,
    sync::{Arc, Weak},
    time::UNIX_EPOCH,
};

use log::{debug, trace, warn};
use rand::{
    prelude::{IteratorRandom, SliceRandom},
    rngs::OsRng,
};

use rand::Rng;
use smol::lock::RwLock;
use url::Url;

use super::super::{
    p2p::{P2p, P2pPtr},
    settings::SettingsPtr,
};
use crate::{
    net::{
        connector::Connector,
        protocol::ProtocolVersion,
        session::{Session, SessionWeakPtr},
    },
    system::{
        sleep, LazyWeak, StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr, Subscription,
    },
    Error, Result,
};

//// Probe random peers on the greylist. If a peer is responsive, update the last_seen field and
//// add it to the whitelist. If a node does not respond, remove it from the greylist.
//// Called periodically.
// NOTE: in monero this is called "greylist housekeeping" but that's a bit verbose.
struct GreylistRefinery {
    p2p: P2pPtr,
    process: StoppableTaskPtr,
    session: SessionWeakPtr,
}

impl GreylistRefinery {
    fn new(p2p: P2pPtr, session: SessionWeakPtr) -> Arc<Self> {
        Arc::new(Self { p2p, process: StoppableTask::new(), session })
    }

    async fn start(self: Arc<Self>) {
        let ex = self.p2p.executor();
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

    //// Randomly select a peer on the greylist and probe it.
    //// TODO: This frequency of this call can be set in net::Settings.
    async fn run(self: Arc<Self>) {
        debug!(target: "net::refinery::run()", "START");
        loop {
            let hosts = self.p2p.hosts();
            let session = self.p2p.session_outbound();

            if hosts.is_empty_greylist().await {
                warn!(target: "net::refinery::run()", "Greylist is empty. Aborting");
                break
            } else {
                debug!(target: "net::refinery::run()", "Starting refinery process");
                // Randomly select an entry from the greylist.
                let greylist = hosts.greylist.read().await;
                let position = rand::thread_rng().gen_range(0..greylist.len());
                let entry = &greylist[position];
                let url = &entry.0;

                if ping_node(url, self.p2p.clone(), self.session.clone()).await {
                    let whitelist = hosts.whitelist.read().await;
                    // Remove oldest element if the whitelist reaches max size.
                    if whitelist.len() == 1000 {
                        // Last element in vector should have the oldest timestamp.
                        // This should never crash as only returns None when whitelist len() == 0.
                        let mut whitelist = hosts.whitelist.write().await;
                        let entry = whitelist.pop().unwrap();
                        debug!(target: "net::refinery::run()", "Whitelist reached max size. Removed host {}", entry.0);
                    }

                    // Peer is responsive. Update last_seen and add it to the whitelist.
                    let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();

                    // Append to the whitelist.
                    debug!(target: "net::refinery::run()", "Adding peer {} to whitelist", url);
                    let mut whitelist = hosts.whitelist.write().await;
                    whitelist.push((url.clone(), last_seen));

                    // Sort whitelist by last_seen.
                    whitelist.sort_unstable_by_key(|entry| entry.1);

                    // Remove whitelisted peer from the greylist.
                    debug!(target: "net::refinery::run()", "Removing whitelisted peer {} from greylist", url);
                    let mut greylist = hosts.greylist.write().await;
                    greylist.remove(position);
                } else {
                    let mut greylist = hosts.greylist.write().await;
                    greylist.remove(position);
                    debug!(target: "net::refinery::run()", "Peer {} is not response. Removed from greylist", url);
                }
            }

            // TODO: create a custom net setting for this timer
            debug!(target: "net::greylist_refinery::run()", "Sleeping...");
            sleep(self.p2p.settings().outbound_peer_discovery_attempt_time).await;
        }
    }
}

// Ping a node to check it's online.
// TODO: make this an actual ping-pong method, rather than a version exchange.
pub async fn ping_node(addr: &Url, p2p: P2pPtr, session: SessionWeakPtr) -> bool {
    let connector = Connector::new(p2p.settings(), session.clone());
    let outbound_session = p2p.session_outbound();

    debug!(target: "net::refinery::ping_node()", "Attempting to connect to {}", addr);
    match connector.connect(addr).await {
        Ok((_url, channel)) => {
            debug!(target: "net::refinery::ping_node()", "Connected successfully!");
            let proto_ver = ProtocolVersion::new(channel.clone(), p2p.settings()).await;

            let handshake_task = outbound_session.perform_handshake_protocols(
                proto_ver,
                channel.clone(),
                p2p.executor(),
            );

            channel.clone().start(p2p.executor());

            match handshake_task.await {
                Ok(()) => {
                    debug!(target: "net::refinery::ping_node()", "Handshake success! Stopping channel.");
                    channel.stop().await;
                    true
                }
                Err(e) => {
                    debug!(target: "net::refinery::ping_node()", "Handshake failure! {}", e);
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
