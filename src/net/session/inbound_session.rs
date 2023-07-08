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

//! Inbound connections session. Manages the creation of inbound sessions.
//! Used to create an inbound session and start and stop the session.
//!
//! Class consists of 3 pointers: a weak pointer to the p2p parent class,
//! an acceptor pointer, and a stoppable task pointer. Using a weak pointer
//! to P2P allows us to avoid circular dependencies.

use std::collections::HashMap;

use async_std::sync::{Arc, Mutex, Weak};
use async_trait::async_trait;
use log::{error, info};
use smol::Executor;
use url::Url;

use super::{
    super::{
        acceptor::{Acceptor, AcceptorPtr},
        channel::{ChannelInfo, ChannelPtr},
        p2p::{DnetInfo, P2p, P2pPtr},
    },
    Session, SessionBitFlag, SESSION_INBOUND,
};
use crate::{
    system::{StoppableTask, StoppableTaskPtr},
    Error, Result,
};

pub type InboundSessionPtr = Arc<InboundSession>;

/// dnet info for an inbound connection
#[derive(Clone)]
pub struct InboundInfo {
    /// Remote address
    pub addr: Option<Url>,
    /// Channel info
    pub channel: Option<ChannelInfo>,
}

impl InboundInfo {
    async fn dnet_info(&self, p2p: P2pPtr) -> Option<Self> {
        let Some(ref addr) = self.addr else {
            return None
        };

        let Some(chan) = p2p.channels().lock().await.get(&addr).cloned() else {
            return None
        };

        Some(Self { addr: self.addr.clone(), channel: Some(chan.dnet_info().await) })
    }
}

/// Defines inbound connections session
pub struct InboundSession {
    p2p: Weak<P2p>,
    acceptors: Mutex<Vec<AcceptorPtr>>,
    accept_tasks: Mutex<Vec<StoppableTaskPtr>>,
    connect_infos: Mutex<Vec<HashMap<Url, InboundInfo>>>,
}

impl InboundSession {
    /// Create a new inbound session
    pub fn new(p2p: Weak<P2p>) -> InboundSessionPtr {
        Arc::new(Self {
            p2p,
            acceptors: Mutex::new(vec![]),
            accept_tasks: Mutex::new(vec![]),
            connect_infos: Mutex::new(vec![]),
        })
    }

    /// Starts the inbound session. Begins by accepting connections and fails
    /// if the addresses are not configured. Then runs the channel subscription
    /// loop.
    pub async fn start(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        if self.p2p().settings().inbound_addrs.is_empty() {
            info!(target: "net::inbound_session", "[P2P] Not configured for inbound connections.");
            return Ok(())
        }

        // Activate mutex lock on accept tasks.
        let mut accept_tasks = self.accept_tasks.lock().await;

        for (index, accept_addr) in self.p2p().settings().inbound_addrs.iter().enumerate() {
            self.clone().start_accept_session(index, accept_addr.clone(), ex.clone()).await?;

            let task = StoppableTask::new();

            task.clone().start(
                self.clone().channel_sub_loop(index, ex.clone()),
                // Ignore stop handler
                |_| async {},
                Error::NetworkServiceStopped,
                ex.clone(),
            );

            self.connect_infos.lock().await.push(HashMap::new());
            accept_tasks.push(task);
        }

        Ok(())
    }

    /// Stops the inbound session.
    pub async fn stop(&self) {
        let acceptors = &*self.acceptors.lock().await;
        for acceptor in acceptors {
            acceptor.stop().await;
        }

        let accept_tasks = &*self.accept_tasks.lock().await;
        for accept_task in accept_tasks {
            accept_task.stop().await;
        }
    }

    /// Start accepting connections for inbound session.
    async fn start_accept_session(
        self: Arc<Self>,
        index: usize,
        accept_addr: Url,
        ex: Arc<Executor<'_>>,
    ) -> Result<()> {
        info!(target: "net::inbound_session", "[P2P] Starting Inbound session #{} on {}", index, accept_addr);
        // Generate a new acceptor for this inbound session
        let acceptor = Acceptor::new(Mutex::new(None));
        let parent = Arc::downgrade(&self);
        *acceptor.session.lock().await = Some(Arc::new(parent));

        // Start listener
        let result = acceptor.clone().start(accept_addr, ex).await;
        if let Err(e) = result.clone() {
            error!(target: "net::inbound_session", "[P2P] Error starting listener #{}: {}", index, e);
            acceptor.stop().await;
        } else {
            self.acceptors.lock().await.push(acceptor);
        }

        result
    }

    /// Wait for all new channels created by the acceptor and call setup_channel() on them.
    async fn channel_sub_loop(self: Arc<Self>, index: usize, ex: Arc<Executor<'_>>) -> Result<()> {
        let channel_sub = self.acceptors.lock().await[index].clone().subscribe().await;

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
        info!(target: "net::inbound_session", "[P2P] Connected Inbound #{} [{}]", index, channel.address());
        self.register_channel(channel.clone(), ex.clone()).await?;

        let addr = channel.address().clone();

        self.connect_infos.lock().await[index]
            .insert(addr.clone(), InboundInfo { addr: Some(addr.clone()), channel: None });

        let stop_sub = channel.subscribe_stop().await?;
        stop_sub.receive().await;

        self.connect_infos.lock().await[index].remove(&addr);

        Ok(())
    }
}

/// Dnet information for the inbound session
pub struct InboundDnet {
    /// Slot information
    pub slots: Vec<Option<InboundInfo>>,
}

#[async_trait]
impl Session for InboundSession {
    fn p2p(&self) -> P2pPtr {
        self.p2p.upgrade().unwrap()
    }

    fn type_id(&self) -> SessionBitFlag {
        SESSION_INBOUND
    }

    async fn dnet_info(&self) -> DnetInfo {
        let mut slots = vec![];

        for listen_addr in (*self.connect_infos.lock().await).iter() {
            for slot in listen_addr.values() {
                slots.push(slot.dnet_info(self.p2p()).await);
            }
        }

        DnetInfo::Inbound(InboundDnet { slots })
    }
}
