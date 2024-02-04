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

//! Inbound connections session. Manages the creation of inbound sessions.
//! Used to create an inbound session and start and stop the session.
//!
//! Class consists of 3 pointers: a weak pointer to the p2p parent class,
//! an acceptor pointer, and a stoppable task pointer. Using a weak pointer
//! to P2P allows us to avoid circular dependencies.

use std::{collections::HashMap, sync::Arc, time::UNIX_EPOCH};

use async_trait::async_trait;
use log::{debug, error, info, warn};
use smol::{lock::Mutex, Executor};
use url::Url;

use super::{
    super::{
        acceptor::{Acceptor, AcceptorPtr},
        channel::ChannelPtr,
        dnet::{self, dnetev, DnetEvent},
        hosts::refinery::ping_node,
        p2p::{P2p, P2pPtr},
    },
    Session, SessionBitFlag, SESSION_INBOUND,
};
use crate::{
    system::{sleep, LazyWeak, StoppableTask, StoppableTaskPtr, Subscription},
    Error, Result,
};

pub type InboundSessionPtr = Arc<InboundSession>;

/// Defines inbound connections session
pub struct InboundSession {
    pub(in crate::net) p2p: LazyWeak<P2p>,
    acceptors: Mutex<Vec<AcceptorPtr>>,
    accept_tasks: Mutex<Vec<StoppableTaskPtr>>,
    /// Task that periodically checks our external addresses.
    pub(in crate::net) ping_self: Arc<PingSelfProcess>,
}

impl InboundSession {
    /// Create a new inbound session
    pub fn new() -> InboundSessionPtr {
        let self_ = Arc::new(Self {
            p2p: LazyWeak::new(),
            acceptors: Mutex::new(Vec::new()),
            accept_tasks: Mutex::new(Vec::new()),
            ping_self: PingSelfProcess::new(),
        });
        self_.ping_self.session.init(self_.clone());
        self_
    }

    /// Starts the inbound session. Begins by accepting connections and fails
    /// if the addresses are not configured. Then runs the channel subscription
    /// loop.
    pub async fn start(self: Arc<Self>) -> Result<()> {
        if self.p2p().settings().inbound_addrs.is_empty() {
            info!(target: "net::inbound_session", "[P2P] Not configured for inbound connections.");
            return Ok(())
        }

        let ex = self.p2p().executor();

        // Activate mutex lock on accept tasks.
        let mut accept_tasks = self.accept_tasks.lock().await;

        for (index, accept_addr) in self.p2p().settings().inbound_addrs.iter().enumerate() {
            // First initialize an Acceptor and its Subscriber.
            let parent = Arc::downgrade(&self);
            let acceptor = Acceptor::new(parent);

            // Now start the Subscriber. The Subscriber will return a Channel once it has been
            // prepared by the Acceptor.
            let channel_sub = acceptor.clone().subscribe().await;

            // Then start listening for a Channel returned by the Subscriber. Call setup_channel()
            // to register the Channel when it has been received.
            let task = StoppableTask::new();
            task.clone().start(
                self.clone().channel_sub_loop(channel_sub, index, ex.clone()),
                // Ignore stop handler
                |_| async {},
                Error::NetworkServiceStopped,
                ex.clone(),
            );

            accept_tasks.push(task);

            // Finally, run the Acceptor to start accepting inbound connections. Only when
            // the Subscriber has been set up can we safely do this.
            self.clone()
                .start_accept_session(index, accept_addr.clone(), acceptor, ex.clone())
                .await?;
        }

        debug!(target: "net::inbound_session", "Starting ping_self process");
        self.ping_self.clone().start().await;

        Ok(())
    }

    /// Stops the inbound session.
    pub async fn stop(&self) {
        if self.p2p().settings().inbound_addrs.is_empty() {
            info!(target: "net::inbound_session", "[P2P] Not configured for inbound connections.");
            return
        }

        let acceptors = &*self.acceptors.lock().await;
        for acceptor in acceptors {
            acceptor.stop().await;
        }

        let accept_tasks = &*self.accept_tasks.lock().await;
        for accept_task in accept_tasks {
            accept_task.stop().await;
        }

        debug!(target: "net::inbound_session", "Stopping ping_self process");
        self.ping_self.clone().stop().await;
    }

    /// Start accepting connections for inbound session.
    async fn start_accept_session(
        self: Arc<Self>,
        index: usize,
        accept_addr: Url,
        acceptor: AcceptorPtr,
        ex: Arc<Executor<'_>>,
    ) -> Result<()> {
        info!(target: "net::inbound_session", "[P2P] Starting Inbound session #{} on {}", index, accept_addr);
        // Start listener
        let result = acceptor.clone().start(accept_addr, ex).await;
        if let Err(e) = &result {
            error!(target: "net::inbound_session", "[P2P] Error starting listener #{}: {}", index, e);
            acceptor.stop().await;
        } else {
            self.acceptors.lock().await.push(acceptor);
        }

        result
    }

    /// Wait for all new channels created by the acceptor and call setup_channel() on them.
    async fn channel_sub_loop(
        self: Arc<Self>,
        channel_sub: Subscription<Result<ChannelPtr>>,
        index: usize,
        ex: Arc<Executor<'_>>,
    ) -> Result<()> {
        loop {
            let channel = channel_sub.receive().await?;
            // Spawn a detached task to process the channel.
            // This will just perform the channel setup then exit.
            ex.spawn(self.clone().setup_channel(index, channel, ex.clone())).detach();
        }
    }

    /// Registers the channel. First performs a network handshake and starts the channel.
    /// Then starts sending keep-alive and address messages across the channel.
    async fn setup_channel(
        self: Arc<Self>,
        index: usize,
        channel: ChannelPtr,
        ex: Arc<Executor<'_>>,
    ) -> Result<()> {
        info!(
            target: "net::inbound_session::setup_channel",
            "[P2P] Connected Inbound #{} [{}]", index, channel.address(),
        );

        dnetev!(self, InboundConnected, {
            addr: channel.info.addr.clone(),
            channel_id: channel.info.id,
        });

        let stop_sub = channel.subscribe_stop().await?;

        self.register_channel(channel.clone(), ex.clone()).await?;

        stop_sub.receive().await;

        self.p2p().remove(channel.clone()).await;

        debug!(
            target: "net::inbound_session::setup_channel()",
            "Received stop_sub, channel removed from P2P",
        );

        dnetev!(self, InboundDisconnected, {
            addr: channel.info.addr.clone(),
            channel_id: channel.info.id,
        });

        Ok(())
    }
}

#[async_trait]
impl Session for InboundSession {
    fn p2p(&self) -> P2pPtr {
        self.p2p.upgrade()
    }

    fn type_id(&self) -> SessionBitFlag {
        SESSION_INBOUND
    }
}

/// Periodically try to do a version exchange with our own external
/// addresses. If the version exchange is successful, take a timestamp and
/// save it along with the external addresses. Each address along with its
/// timestamp (the `last_seen` data field) is sent in to other nodes in
/// ProtocolAddr and ProtocolSeed.
///
/// On first run, PingSelfProcess will immediately conduct a version exchange
/// with our external addresses, and if successful update the last_seen
/// field. The process will wait [TODO: ping_self_interval) before retrying.
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
pub struct PingSelfProcess {
    process: StoppableTaskPtr,
    session: LazyWeak<InboundSession>,
    pub(in crate::net) addrs: Mutex<HashMap<Url, u64>>,
}

impl PingSelfProcess {
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

            // Only proceed if the external address is not configured.
            if external_addrs.is_empty() {
                current_attempt += 1;
                continue
            }

            for addr in external_addrs.iter() {
                debug!(target: "net::inbound_session::ping_self",
                "Attempting a version exchange addr={}", addr);

                if ping_node(addr.clone(), self.session().p2p()).await {
                    debug!(target: "net::inbound_session::ping_self",
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
                    warn!(target: "net::inbound_session::ping_self",
                    "Version exchange failed! addr={}", addr);
                }
            }
            current_attempt += 1;
        }
    }

    fn session(&self) -> InboundSessionPtr {
        self.session.upgrade()
    }
}
