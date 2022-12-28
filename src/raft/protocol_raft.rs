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

use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use chrono::Utc;
use darkfi_serial::serialize;
use log::debug;
use rand::{rngs::OsRng, RngCore};
use smol::Executor;

use super::primitives::{NetMsg, NetMsgMethod, NodeId, NodeIdMsg};
use crate::{net, Result};

pub struct ProtocolRaft {
    id: NodeId,
    jobsman: net::ProtocolJobsManagerPtr,
    notify_queue_sender: smol::channel::Sender<NetMsg>,
    msg_sub: net::MessageSubscription<NetMsg>,
    p2p: net::P2pPtr,
    seen_msgs: Arc<Mutex<HashMap<String, i64>>>,
    channel: net::ChannelPtr,
}

impl ProtocolRaft {
    pub async fn init(
        id: NodeId,
        channel: net::ChannelPtr,
        notify_queue_sender: smol::channel::Sender<NetMsg>,
        p2p: net::P2pPtr,
        seen_msgs: Arc<Mutex<HashMap<String, i64>>>,
    ) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<NetMsg>().await;

        let msg_sub = channel.subscribe_msg::<NetMsg>().await.expect("Missing NetMsg dispatcher!");

        Arc::new(Self {
            id,
            notify_queue_sender,
            msg_sub,
            jobsman: net::ProtocolJobsManager::new("ProtocolRaft", channel.clone()),
            p2p,
            seen_msgs,
            channel,
        })
    }

    async fn handle_receive_msg(self: Arc<Self>) -> Result<()> {
        debug!(target: "protocol_raft", "ProtocolRaft::handle_receive_msg() [START]");

        // on initialization send a NodeIdMsg
        let random_id = OsRng.next_u64();
        let node_id_msg = serialize(&NodeIdMsg { id: self.id.clone() });
        let net_msg = NetMsg {
            id: random_id,
            recipient_id: None,
            payload: node_id_msg.to_vec(),
            method: NetMsgMethod::NodeIdMsg,
        };
        {
            self.seen_msgs.lock().await.insert(random_id.to_string(), Utc::now().timestamp());
        }
        self.channel.send(net_msg).await?;

        loop {
            let msg = self.msg_sub.receive().await?;

            debug!(
            target: "protocol_raft",
            "ProtocolRaft::handle_receive_msg() received id: {:?} method {:?}",
            &msg.id, &msg.method
            );

            {
                let mut msgs = self.seen_msgs.lock().await;
                if msgs.contains_key(&msg.id.to_string()) {
                    continue
                }
                msgs.insert(msg.id.to_string(), chrono::Utc::now().timestamp());
            }

            let msg = (*msg).clone();
            self.p2p.broadcast(msg.clone()).await?;

            // check if the local node and recipient id are equal
            if let Some(recipient_id) = &msg.recipient_id {
                if &self.id != recipient_id {
                    continue
                }
            }

            self.notify_queue_sender.send(msg).await?;
        }
    }
}

#[async_trait]
impl net::ProtocolBase for ProtocolRaft {
    /// Starts ping-pong keep-alive messages exchange. Runs ping-pong in the
    /// protocol task manager, then queues the reply. Sends out a ping and
    /// waits for pong reply. Waits for ping and replies with a pong.
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "protocol_raft", "ProtocolRaft::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_msg(), executor.clone()).await;
        debug!(target: "protocol_raft", "ProtocolRaft::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolRaft"
    }
}

impl net::Message for NetMsg {
    fn name() -> &'static str {
        "netmsg"
    }
}
