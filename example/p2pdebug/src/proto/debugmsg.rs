/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::sync::Arc;

use async_channel::Sender;
use async_executor::Executor;
use async_std::sync::Mutex;
use async_trait::async_trait;
use fxhash::FxHashSet;
use log::debug;

use darkfi::{
    net,
    util::serial::{SerialDecodable, SerialEncodable},
    Result,
};

pub type DebugmsgId = u32;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Debugmsg {
    pub id: DebugmsgId,
    pub message: String,
}

impl net::Message for Debugmsg {
    fn name() -> &'static str {
        "debugmsg"
    }
}

pub struct SeenDebugmsgIds {
    ids: Mutex<FxHashSet<DebugmsgId>>,
}

pub type SeenDebugmsgIdsPtr = Arc<SeenDebugmsgIds>;

impl SeenDebugmsgIds {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { ids: Mutex::new(FxHashSet::default()) })
    }

    pub async fn add_seen(&self, id: u32) {
        self.ids.lock().await.insert(id);
    }

    pub async fn is_seen(&self, id: u32) -> bool {
        self.ids.lock().await.contains(&id)
    }
}

pub struct ProtocolDebugmsg {
    notify_queue_sender: Sender<Arc<Debugmsg>>,
    debugmsg_sub: net::MessageSubscription<Debugmsg>,
    jobsman: net::ProtocolJobsManagerPtr,
    seen_ids: SeenDebugmsgIdsPtr,
    p2p: net::P2pPtr,
}

#[async_trait]
impl net::ProtocolBase for ProtocolDebugmsg {
    /// Starts ping-pong keep-alive messages exchange. Runs ping-pong in the
    /// protocol task manager, then queues the reply. Sends out a ping and
    /// waits for pong reply. Waits for ping and replies with a pong.
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "p2pdbg", "Protocoldebugmsg::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_debugmsg(), executor.clone()).await;
        debug!(target: "p2pdbg", "ProtocolDebugmsg::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Protocoldebugmsg"
    }
}

impl ProtocolDebugmsg {
    pub async fn init(
        channel: net::ChannelPtr,
        notify_queue_sender: Sender<Arc<Debugmsg>>,
        seen_ids: SeenDebugmsgIdsPtr,
        p2p: net::P2pPtr,
    ) -> net::ProtocolBasePtr {
        let message_subsystem = channel.get_message_subsystem();
        message_subsystem.add_dispatch::<Debugmsg>().await;

        let sub = channel.subscribe_msg::<Debugmsg>().await.expect("Missing Debugmsg dispatcher!");

        Arc::new(Self {
            notify_queue_sender,
            debugmsg_sub: sub,
            jobsman: net::ProtocolJobsManager::new("DebugmsgProtocol", channel),
            seen_ids,
            p2p,
        })
    }

    async fn handle_receive_debugmsg(self: Arc<Self>) -> Result<()> {
        debug!(target: "p2pdbg", "ProtocolDebugmsg::handle_receive_debugmsg() [START]");

        loop {
            let debugmsg = self.debugmsg_sub.receive().await?;

            debug!(target: "p2pdbg", "ProtocolDebugmsg::handle_receive_debugmsg() received {:?}", debugmsg);

            // Do we already have this message?
            if self.seen_ids.is_seen(debugmsg.id).await {
                continue
            }

            self.seen_ids.add_seen(debugmsg.id).await;

            // If not, then broadcast to network.
            let debugmsg_copy = (*debugmsg).clone();
            self.p2p.broadcast(debugmsg_copy).await?;

            self.notify_queue_sender
                .send(debugmsg)
                .await
                .expect("notify_queue_sender send failed!");
        }
    }
}
