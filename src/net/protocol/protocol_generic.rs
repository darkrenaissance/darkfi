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

use std::{clone::Clone, collections::HashMap, sync::Arc};

use async_trait::async_trait;
use log::debug;
use smol::{
    channel::{Receiver, Sender},
    lock::RwLock,
    Executor,
};

use super::{
    super::{
        channel::ChannelPtr, message::Message, message_publisher::MessageSubscription,
        session::SessionBitFlag,
    },
    protocol_base::{ProtocolBase, ProtocolBasePtr},
    protocol_jobs_manager::{ProtocolJobsManager, ProtocolJobsManagerPtr},
    P2pPtr,
};
use crate::{
    system::{StoppableTask, StoppableTaskPtr},
    Error, Result,
};

/// Defines generic messages protocol action signal.
#[derive(Debug)]
pub enum ProtocolGenericAction {
    /// Broadcast message to rest nodes
    Broadcast,
    /// Skip message broadcast
    Skip,
    /// Stop the channel entirely
    Stop,
}

pub type ProtocolGenericHandlerPtr<M> = Arc<ProtocolGenericHandler<M>>;

/// Defines a handler for generic protocol messages, consisting
/// of a message receiver, action signal senders mapped by each
/// channel ID, and a stoppable task to run the handler in the
/// background.
pub struct ProtocolGenericHandler<M: Message + Clone> {
    // Since smol channels close if all senders or all receivers
    // get dropped, we will keep one here to remain alive with the
    // handler.
    sender: Sender<(u32, M)>,
    pub receiver: Receiver<(u32, M)>,
    senders: RwLock<HashMap<u32, Sender<ProtocolGenericAction>>>,
    pub task: StoppableTaskPtr,
}

impl<M: Message + Clone> ProtocolGenericHandler<M> {
    pub async fn new(
        p2p: &P2pPtr,
        name: &'static str,
        session: SessionBitFlag,
    ) -> ProtocolGenericHandlerPtr<M> {
        let (sender, receiver) = smol::channel::unbounded::<(u32, M)>();
        let senders = RwLock::new(HashMap::new());
        let task = StoppableTask::new();
        let handler = Arc::new(Self { sender, receiver, senders, task });

        let _handler = handler.clone();
        p2p.protocol_registry()
            .register(session, move |channel, p2p| {
                let handler = _handler.clone();
                async move { ProtocolGeneric::init(channel, name, handler, p2p).await.unwrap() }
            })
            .await;

        handler
    }

    /// Registers a new channel sender to the handler map.
    /// Additionally, looks for stale(closed) channels and prunes then from it.
    async fn register_channel_sender(&self, channel: u32, sender: Sender<ProtocolGenericAction>) {
        let mut lock = self.senders.write().await;
        lock.insert(channel, sender);
        let mut stale = vec![];
        for (channel, sender) in lock.iter() {
            if sender.is_closed() {
                stale.push(*channel);
            }
        }
        for channel in stale {
            lock.remove(&channel);
        }
        drop(lock);
    }

    /// Sends provided protocol generic action to requested channel, if it exists.
    pub async fn send_action(&self, channel: u32, action: ProtocolGenericAction) {
        debug!(
            target: "net::protocol_generic::ProtocolGenericHandler::send_action",
            "Sending action {action:?} to channel {channel}..."
        );
        let mut lock = self.senders.write().await;
        let Some(sender) = lock.get(&channel) else {
            debug!(
                target: "net::protocol_generic::ProtocolGenericHandler::send_action",
                "Channel wasn't found."
            );
            drop(lock);
            return
        };
        if let Err(e) = sender.send(action).await {
            debug!(
                target: "net::protocol_generic::ProtocolGenericHandler::send_action",
                "Channel {channel} send fail: {e}"
            );
            lock.remove(&channel);
        };
        drop(lock);
    }
}

/// Defines generic messages protocol.
pub struct ProtocolGeneric<M: Message + Clone> {
    /// The P2P channel message subcription
    msg_sub: MessageSubscription<M>,
    /// The generic message smol channel sender
    sender: Sender<(u32, M)>,
    /// Action signal smol channel receiver
    receiver: Receiver<ProtocolGenericAction>,
    /// The P2P channel the protocol is serving
    channel: ChannelPtr,
    /// Pointer to the whole P2P instance
    p2p: P2pPtr,
    /// Pointer to the protocol job manager
    jobsman: ProtocolJobsManagerPtr,
}

impl<M: Message + Clone> ProtocolGeneric<M> {
    /// Initialize a new generic protocol.
    pub async fn init(
        channel: ChannelPtr,
        name: &'static str,
        handler: ProtocolGenericHandlerPtr<M>,
        p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        debug!(
            target: "net::protocol_generic::init",
            "Adding generic protocol for message {name} to the protocol registry"
        );
        // Add the message dispatcher
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<M>().await;

        // Create the message subscription
        let msg_sub = channel.subscribe_msg::<M>().await?;

        // Create a new sender channel
        let (action_sender, receiver) = smol::channel::bounded(1);
        handler.register_channel_sender(channel.info.id, action_sender).await;

        Ok(Arc::new(Self {
            msg_sub,
            sender: handler.sender.clone(),
            receiver,
            channel: channel.clone(),
            p2p,
            jobsman: ProtocolJobsManager::new(name, channel),
        }))
    }

    /// Runs the message queue. We listen for the specified structure message,
    /// and when one is received, we send it to our smol channel. Afterwards,
    /// we wait for an action signal, specifying whether or not we should
    /// propagate the message to rest nodes or skip it.
    async fn handle_receive_message(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "net::protocol_generic::handle_receive_message",
            "START"
        );
        let exclude_list = vec![self.channel.address().clone()];
        loop {
            // Wait for a new message
            let msg = match self.msg_sub.receive().await {
                Ok(m) => m,
                Err(e) => {
                    debug!(
                        target: "net::protocol_generic::handle_receive_message",
                        "[{}] recv fail: {e}", self.jobsman.clone().name()
                    );
                    continue
                }
            };

            let msg_copy = (*msg).clone();

            // Send the message across the smol channel
            if let Err(e) = self.sender.send((self.channel.info.id, msg_copy.clone())).await {
                debug!(
                    target: "net::protocol_generic::handle_receive_message",
                    "[{}] sending to channel fail: {e}", self.jobsman.clone().name()
                );
                continue
            }

            // Wait for action signal
            let action = match self.receiver.recv().await {
                Ok(a) => a,
                Err(e) => {
                    debug!(
                        target: "net::protocol_generic::handle_receive_message",
                        "[{}] action signal recv fail: {e}", self.jobsman.clone().name()
                    );
                    continue
                }
            };

            // Handle action signal
            match action {
                ProtocolGenericAction::Broadcast => {
                    self.p2p.broadcast_with_exclude(&msg_copy, &exclude_list).await
                }
                ProtocolGenericAction::Skip => {
                    debug!(
                        target: "net::protocol_generic::handle_receive_message",
                        "[{}] Skip action signal received.", self.jobsman.clone().name()
                    );
                }
                ProtocolGenericAction::Stop => {
                    self.channel.stop().await;
                    return Err(Error::ChannelStopped)
                }
            }
        }
    }
}

#[async_trait]
impl<M: Message + Clone> ProtocolBase for ProtocolGeneric<M> {
    async fn start(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::protocol_generic::start", "START");
        self.jobsman.clone().start(ex.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_message(), ex).await;
        debug!(target: "net::protocol_generic::start", "END");
        Ok(())
    }

    fn name(&self) -> &'static str {
        self.jobsman.clone().name()
    }
}
