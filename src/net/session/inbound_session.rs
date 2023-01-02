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
use std::collections::HashMap;

use async_std::sync::{Arc, Mutex, Weak};
use async_trait::async_trait;
use log::{error, info};
use serde_json::json;
use smol::Executor;
use url::Url;

use crate::{
    system::{StoppableTask, StoppableTaskPtr},
    Error, Result,
};

use super::{
    super::{Acceptor, AcceptorPtr, ChannelPtr, P2p},
    Session, SessionBitflag, SESSION_INBOUND,
};

struct InboundInfo {
    channel: ChannelPtr,
}

impl InboundInfo {
    async fn get_info(&self) -> serde_json::Value {
        self.channel.get_info().await
    }
}

/// Defines inbound connections session.
pub struct InboundSession {
    p2p: Weak<P2p>,
    acceptors: Mutex<Vec<AcceptorPtr>>,
    accept_tasks: Mutex<Vec<StoppableTaskPtr>>,
    connect_infos: Mutex<Vec<HashMap<Url, InboundInfo>>>,
}

impl InboundSession {
    /// Create a new inbound session.
    pub async fn new(p2p: Weak<P2p>) -> Arc<Self> {
        Arc::new(Self {
            p2p,
            acceptors: Mutex::new(Vec::new()),
            accept_tasks: Mutex::new(Vec::new()),
            connect_infos: Mutex::new(Vec::new()),
        })
    }

    /// Starts the inbound session. Begins by accepting connections and fails if
    /// the addresses are not configured. Then runs the channel subscription
    /// loop.
    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        if self.p2p().settings().inbound.is_empty() {
            info!(target: "net::inbound_session", "Not configured for accepting incoming connections.");
            return Ok(())
        }

        // Activate mutex lock on accept tasks.
        let mut accept_tasks = self.accept_tasks.lock().await;

        for (index, accept_addr) in self.p2p().settings().inbound.iter().enumerate() {
            self.clone().start_accept_session(index, accept_addr.clone(), executor.clone()).await?;

            let task = StoppableTask::new();

            task.clone().start(
                self.clone().channel_sub_loop(index, executor.clone()),
                // Ignore stop handler
                |_| async {},
                Error::NetworkServiceStopped,
                executor.clone(),
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
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        info!(target: "net::inbound_session", "#{} starting inbound session on {}", index, accept_addr);
        // Generate a new acceptor for this inbound session
        let acceptor = Acceptor::new(Mutex::new(None));
        let parent = Arc::downgrade(&self);
        *acceptor.session.lock().await = Some(Arc::new(parent));

        // Start listener
        let result = acceptor.clone().start(accept_addr, executor).await;
        if let Err(err) = result.clone() {
            error!(target: "net::inbound_session", "#{} error starting listener: {}", index, err);
        }

        self.acceptors.lock().await.push(acceptor);

        result
    }

    /// Wait for all new channels created by the acceptor and call
    /// setup_channel() on them.
    async fn channel_sub_loop(
        self: Arc<Self>,
        index: usize,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let channel_sub = self.acceptors.lock().await[index].clone().subscribe().await;
        loop {
            let channel = channel_sub.receive().await?;
            // Spawn a detached task to process the channel
            // This will just perform the channel setup then exit.
            executor.spawn(self.clone().setup_channel(index, channel, executor.clone())).detach();
        }
    }

    /// Registers the channel. First performs a network handshake and starts the
    /// channel. Then starts sending keep-alive and address messages across the
    /// channel.
    async fn setup_channel(
        self: Arc<Self>,
        index: usize,
        channel: ChannelPtr,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        info!(target: "net::inbound_session", "#{} connected inbound [{}]", index, channel.address());

        self.clone().register_channel(channel.clone(), executor.clone()).await?;

        self.manage_channel_for_get_info(index, channel).await;

        Ok(())
    }

    async fn manage_channel_for_get_info(&self, index: usize, channel: ChannelPtr) {
        let key = channel.address();
        self.connect_infos.lock().await[index]
            .insert(key.clone(), InboundInfo { channel: channel.clone() });

        let stop_sub = channel.subscribe_stop().await;

        if stop_sub.is_ok() {
            stop_sub.unwrap().receive().await;
        }

        self.connect_infos.lock().await[index].remove(&key);
    }
}

#[async_trait]
impl Session for InboundSession {
    async fn get_info(&self) -> serde_json::Value {
        let mut infos = HashMap::new();
        for (index, accept_addr) in self.p2p().settings().inbound.iter().enumerate() {
            let connect_infos = &self.connect_infos.lock().await[index];
            for (addr, info) in connect_infos {
                let json_addr = json!({ "accept_addr": accept_addr });
                let info = vec![json_addr, info.get_info().await];
                infos.insert(addr.to_string(), info);
            }
        }
        json!({
            "connected": infos,
        })
    }

    fn p2p(&self) -> Arc<P2p> {
        self.p2p.upgrade().unwrap()
    }

    fn type_id(&self) -> SessionBitflag {
        SESSION_INBOUND
    }
}
