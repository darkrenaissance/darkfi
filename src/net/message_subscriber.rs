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

use std::{any::Any, collections::HashMap, io::Cursor, sync::Arc};

use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use log::{debug, warn};
use rand::{rngs::OsRng, Rng};
use smol::lock::Mutex;

use super::message::Message;
use crate::{Error, Result};

/// 64-bit identifier for message subscription.
pub type MessageSubscriptionId = u64;
type MessageResult<M> = Result<Arc<M>>;

/// A dispatcher that is unique to every [`Message`].
/// Maintains a list of subscribers that are subscribed to that
/// unique Message type and handles sending messages across these
/// subscriptions.
#[derive(Debug)]
struct MessageDispatcher<M: Message> {
    subs: Mutex<HashMap<MessageSubscriptionId, smol::channel::Sender<MessageResult<M>>>>,
}

impl<M: Message> MessageDispatcher<M> {
    /// Create a new message dispatcher
    fn new() -> Self {
        Self { subs: Mutex::new(HashMap::new()) }
    }

    /// Create a random ID.
    fn random_id() -> MessageSubscriptionId {
        //let mut rng = rand::thread_rng();
        OsRng.gen()
    }

    /// Subscribe to a channel.
    /// Assigns a new ID and adds it to the list of subscribers.
    pub async fn subscribe(self: Arc<Self>) -> MessageSubscription<M> {
        let (sender, recv_queue) = smol::channel::unbounded();
        // Guard against overwriting
        let mut id = Self::random_id();
        let mut subs = self.subs.lock().await;
        loop {
            if subs.contains_key(&id) {
                id = Self::random_id();
                continue
            }

            subs.insert(id, sender);
            break
        }

        drop(subs);
        MessageSubscription { id, recv_queue, parent: self }
    }

    /// Unsubscribe from a channel.
    /// Removes the associated ID from the subscriber list.
    async fn unsubscribe(&self, sub_id: MessageSubscriptionId) {
        self.subs.lock().await.remove(&sub_id);
    }

    /// Private function to concurrently transmit a message to all subscriber channels.
    /// Automatically clear all inactive channels. Strictly used internally.
    async fn _trigger_all(&self, message: MessageResult<M>) {
        let mut subs = self.subs.lock().await;

        debug!(
            target: "net::message_subscriber::_trigger_all()", "START msg={}({}), subs={}",
            if message.is_ok() { "Ok" } else {"Err"},
            M::NAME, subs.len(),
        );

        let mut futures = FuturesUnordered::new();
        let mut garbage_ids = vec![];

        // Prep the futures for concurrent execution
        for (sub_id, sub) in &*subs {
            let sub_id = *sub_id;
            let sub = sub.clone();
            let message = message.clone();
            futures.push(async move {
                match sub.send(message).await {
                    Ok(res) => Ok((sub_id, res)),
                    Err(err) => Err((sub_id, err)),
                }
            });
        }

        // Start polling
        while let Some(r) = futures.next().await {
            if let Err((sub_id, _err)) = r {
                garbage_ids.push(sub_id);
            }
        }

        // Garbage cleanup
        for sub_id in garbage_ids {
            subs.remove(&sub_id);
        }

        debug!(
            target: "net::message_subscriber::_trigger_all()", "END msg={}({}), subs={}",
            if message.is_ok() { "Ok" } else { "Err" },
            M::NAME, subs.len(),
        );
    }
}

/// Handles message subscriptions through a subscription ID and
/// a receiver channel.
#[derive(Debug)]
pub struct MessageSubscription<M: Message> {
    id: MessageSubscriptionId,
    recv_queue: smol::channel::Receiver<MessageResult<M>>,
    parent: Arc<MessageDispatcher<M>>,
}

impl<M: Message> MessageSubscription<M> {
    /// Start receiving messages.
    pub async fn receive(&self) -> MessageResult<M> {
        match self.recv_queue.recv().await {
            Ok(message) => message,
            Err(e) => panic!("MessageSubscription::receive(): recv_queue failed! {}", e),
        }
    }

    /// Unsubscribe from a message subscription. Must be called manually.
    pub async fn unsubscribe(&self) {
        self.parent.unsubscribe(self.id).await
    }
}

/// Generic interface for the message dispatcher.
#[async_trait]
trait MessageDispatcherInterface: Send + Sync {
    async fn trigger(&self, payload: &[u8]);

    async fn trigger_error(&self, err: Error);

    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

/// Local implementation of the Message Dispatcher Interface
#[async_trait]
impl<M: Message> MessageDispatcherInterface for MessageDispatcher<M> {
    /// Internal function to deserialize data into a message type
    /// and dispatch it across subscriber channels.
    async fn trigger(&self, payload: &[u8]) {
        // Deserialize data into type, send down the pipes.
        let cursor = Cursor::new(payload);
        match M::decode(cursor) {
            Ok(message) => {
                let message = Ok(Arc::new(message));
                self._trigger_all(message).await
            }

            Err(err) => {
                debug!(
                    target: "net::message_subscriber::trigger()",
                    "Unable to decode data. Dropping...: {}",
                    err,
                );
            }
        }
    }

    /// Internal function that sends an error message to all subscriber channels.
    async fn trigger_error(&self, err: Error) {
        self._trigger_all(Err(err)).await;
    }

    /// Converts to `Any` trait. Enables the dynamic modification of static types.
    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}

/// Generic publish/subscribe class that maintains a list of dispatchers.
/// Dispatchers transmit messages to subscribers and are specific to one
/// message type.
#[derive(Default)]
pub struct MessageSubsystem {
    dispatchers: Mutex<HashMap<&'static str, Arc<dyn MessageDispatcherInterface>>>,
}

impl MessageSubsystem {
    /// Create a new message subsystem.
    pub fn new() -> Self {
        Self { dispatchers: Mutex::new(HashMap::new()) }
    }

    /// Add a new dispatcher for specified [`Message`].
    pub async fn add_dispatch<M: Message>(&self) {
        self.dispatchers.lock().await.insert(M::NAME, Arc::new(MessageDispatcher::<M>::new()));
    }

    /// Subscribes to a [`Message`]. Using the Message name, the method
    /// returns the associated `MessageDispatcher` from the list of
    /// dispatchers and calls `subscribe()`.
    pub async fn subscribe<M: Message>(&self) -> Result<MessageSubscription<M>> {
        let dispatcher = self.dispatchers.lock().await.get(M::NAME).cloned();

        let sub = match dispatcher {
            Some(dispatcher) => {
                let dispatcher: Arc<MessageDispatcher<M>> = dispatcher
                    .as_any()
                    .downcast::<MessageDispatcher<M>>()
                    .expect("Multiple messages registered with different names");

                dispatcher.subscribe().await
            }

            None => {
                // Normal return failure here
                return Err(Error::NetworkOperationFailed)
            }
        };

        Ok(sub)
    }

    /// Transmits a payload to a dispatcher.
    /// Returns an error if the payload fails to transmit.
    pub async fn notify(&self, command: &str, payload: &[u8]) -> Result<()> {
        let Some(dispatcher) = self.dispatchers.lock().await.get(command).cloned() else {
            warn!(
                target: "net::message_subscriber::notify",
                "message_subscriber::notify: Command '{}' did not find a dispatcher",
                command,
            );
            return Err(Error::MissingDispatcher)
        };

        dispatcher.trigger(payload).await;
        Ok(())
    }

    /// Concurrently transmits an error message across dispatchers.
    pub async fn trigger_error(&self, err: Error) {
        let mut futures = FuturesUnordered::new();

        let dispatchers = self.dispatchers.lock().await;

        for dispatcher in dispatchers.values() {
            let dispatcher = dispatcher.clone();
            let error = err.clone();
            futures.push(async move { dispatcher.trigger_error(error).await });
        }

        drop(dispatchers);

        while let Some(_r) = futures.next().await {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkfi_serial::{serialize, SerialDecodable, SerialEncodable};

    #[test]
    fn message_subscriber_test() {
        #[derive(SerialEncodable, SerialDecodable)]
        struct MyVersionMessage(pub u32);
        crate::impl_p2p_message!(MyVersionMessage, "verver");

        smol::block_on(async {
            let subsystem = MessageSubsystem::new();
            subsystem.add_dispatch::<MyVersionMessage>().await;

            // Subscribe:
            // 1. Get dispatcher
            // 2. Cast to specific type
            // 3. Do sub, return sub
            let sub = subsystem.subscribe::<MyVersionMessage>().await.unwrap();

            // Receive message and publish:
            // 1. Based on string, lookup relevant dispatcher interface
            // 2. Publish data there
            let msg = MyVersionMessage(110);
            let payload = serialize(&msg);
            subsystem.notify("verver", &payload).await.unwrap();

            // Receive:
            // 1. Do a get easy
            let msg2 = sub.receive().await.unwrap();
            assert_eq!(msg.0, msg2.0);

            // Trigger an error
            subsystem.trigger_error(Error::ChannelStopped).await;

            let msg2 = sub.receive().await;
            assert!(msg2.is_err());

            sub.unsubscribe().await;
        });
    }
}
