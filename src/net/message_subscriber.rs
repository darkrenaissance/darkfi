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

use async_std::sync::Mutex;
use async_trait::async_trait;
use log::{debug, warn};
use rand::Rng;

use crate::{Error, Result};

use super::message::Message;

/// 64bit identifier for message subscription.
pub type MessageSubscriptionId = u64;
type MessageResult<M> = Result<Arc<M>>;

/// Handles message subscriptions through a subscription ID and a receiver
/// channel.
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
            Err(err) => {
                panic!("MessageSubscription::receive() recv_queue failed! {}", err);
            }
        }
    }

    /// Unsubscribe from a message subscription. Must be called manually.
    pub async fn unsubscribe(&self) {
        self.parent.clone().unsubscribe(self.id).await
    }
}

#[async_trait]
/// Generic interface for message dispatcher.
trait MessageDispatcherInterface: Send + Sync {
    async fn trigger(&self, payload: Vec<u8>);

    async fn trigger_error(&self, err: Error);

    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

/// A dispatchers that is unique to every Message. Maintains a list of subscribers that are subscribed to that unique Message type and handles sending messages across these subscriptions.
struct MessageDispatcher<M: Message> {
    subs: Mutex<HashMap<MessageSubscriptionId, smol::channel::Sender<MessageResult<M>>>>,
}

impl<M: Message> MessageDispatcher<M> {
    /// Create a new message dispatcher.
    fn new() -> Self {
        MessageDispatcher { subs: Mutex::new(HashMap::new()) }
    }

    /// Create a random ID.
    fn random_id() -> MessageSubscriptionId {
        let mut rng = rand::thread_rng();
        rng.gen()
    }

    /// Subscribe to a channel. Assigns a new ID and adds it to the list of
    /// subscribers.
    pub async fn subscribe(self: Arc<Self>) -> MessageSubscription<M> {
        let (sender, recvr) = smol::channel::unbounded();
        let sub_id = Self::random_id();
        self.subs.lock().await.insert(sub_id, sender);

        MessageSubscription { id: sub_id, recv_queue: recvr, parent: self }
    }

    /// Unsubcribe from a channel. Removes the associated ID from the subscriber
    /// list.
    async fn unsubscribe(&self, sub_id: MessageSubscriptionId) {
        self.subs.lock().await.remove(&sub_id);
    }

    /// Private function to transmit a message to all subscriber channels. Automatically clear inactive
    /// channels. Used strictly internally.
    async fn _trigger_all(&self, message: MessageResult<M>) {
        debug!(
            target: "net::message_subscriber::_trigger_all()",
            "START, message={}({}), subs={}",
            if message.is_ok() { "Ok" } else { "Err" },
            M::name(),
            self.subs.lock().await.len()
        );
        let mut garbage_ids = Vec::new();

        for (sub_id, sub) in &*self.subs.lock().await {
            match sub.send(message.clone()).await {
                Ok(()) => {}
                Err(_err) => {
                    // Automatically clean out closed channels
                    garbage_ids.push(*sub_id);
                    // panic!("Error returned sending message in notify() call!
                    // {}", err);
                }
            }
        }

        self.collect_garbage(garbage_ids).await;

        debug!(
            target: "net::message_subscriber::_trigger_all()",
            "END, msg={}({}), subs={}",
            if message.is_ok() { "Ok" } else { "Err" },
            M::name(),
            self.subs.lock().await.len()
        );
    }

    /// Remove inactive channels.
    async fn collect_garbage(&self, ids: Vec<MessageSubscriptionId>) {
        let mut subs = self.subs.lock().await;
        for id in &ids {
            subs.remove(id);
        }
    }
}

#[async_trait]
// Local implementation of the Message Dispatcher Interface.
impl<M: Message> MessageDispatcherInterface for MessageDispatcher<M> {
    /// Internal function to deserialize data into a message type and dispatch it across subscriber channels.
    async fn trigger(&self, payload: Vec<u8>) {
        // deserialize data into type
        // send down the pipes
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
                    err
                );
            }
        }
    }

    /// Interal function that sends a Error message to all subscriber channels.
    async fn trigger_error(&self, err: Error) {
        self._trigger_all(Err(err)).await;
    }

    /// Converts to Any trait. Enables the dynamic modification of static types.
    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}

/// Generic publish/subscribe class that maintains a list of dispatchers. Dispatchers transmit
/// messages to subscribers and are specific to one message type.
pub struct MessageSubsystem {
    dispatchers: Mutex<HashMap<&'static str, Arc<dyn MessageDispatcherInterface>>>,
}

impl MessageSubsystem {
    /// Create a new message subsystem.
    pub fn new() -> Self {
        MessageSubsystem { dispatchers: Mutex::new(HashMap::new()) }
    }

    /// Add a new dispatcher for specified Message.
    pub async fn add_dispatch<M: Message>(&self) {
        self.dispatchers.lock().await.insert(M::name(), Arc::new(MessageDispatcher::<M>::new()));
    }

    /// Subscribes to a Message. Using the Message name, the method returns an the associated MessageDispatcher from the list of
    /// dispatchers and calls subscribe().
    pub async fn subscribe<M: Message>(&self) -> Result<MessageSubscription<M>> {
        let dispatcher = self.dispatchers.lock().await.get(M::name()).cloned();

        let sub = match dispatcher {
            Some(dispatcher) => {
                let dispatcher: Arc<MessageDispatcher<M>> = dispatcher
                    .as_any()
                    .downcast::<MessageDispatcher<M>>()
                    .expect("Multiple messages registered with different names");

                dispatcher.subscribe().await
            }
            None => {
                // normall return failure here
                // for now panic
                return Err(Error::NetworkOperationFailed)
            }
        };

        Ok(sub)
    }

    /// Transmits a payload to a dispatcher. Returns an error if the payload
    /// fails to transmit.
    pub async fn notify(&self, command: &str, payload: Vec<u8>) {
        let dispatcher = self.dispatchers.lock().await.get(command).cloned();

        match dispatcher {
            Some(dispatcher) => {
                dispatcher.trigger(payload).await;
            }
            None => {
                warn!(
                    target: "net::message_subscriber::notify()",
                    "Command '{}' did not find a dispatcher",
                    command
                );
            }
        }
    }

    /// Transmits an error message across dispatchers.
    pub async fn trigger_error(&self, err: Error) {
        // TODO: this could be parallelized
        for dispatcher in self.dispatchers.lock().await.values() {
            dispatcher.trigger_error(err.clone()).await;
        }
    }
}

impl Default for MessageSubsystem {
    fn default() -> Self {
        Self::new()
    }
}

/// Test functions for message subsystem.
// This is a test function for the message subsystem code above
// Normall we would use the #[test] macro but cannot since it is async code
// Instead we call it using smol::block_on() in the unit test code after this
// func
#[cfg(test)]
mod tests {
    use super::*;
    use darkfi_serial::{Decodable, Encodable};
    use std::io;

    #[async_std::test]
    async fn message_subscriber_test() {
        struct MyVersionMessage {
            x: u32,
        }

        impl Message for MyVersionMessage {
            fn name() -> &'static str {
                "verver"
            }
        }

        impl Encodable for MyVersionMessage {
            fn encode<S: io::Write>(&self, mut s: S) -> core::result::Result<usize, io::Error> {
                let mut len = 0;
                len += self.x.encode(&mut s)?;
                Ok(len)
            }
        }

        impl Decodable for MyVersionMessage {
            fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
                Ok(Self { x: Decodable::decode(&mut d)? })
            }
        }
        println!("hello");

        let subsystem = MessageSubsystem::new();
        subsystem.add_dispatch::<MyVersionMessage>().await;

        // subscribe
        //   1. get dispatcher
        //   2. cast to specific type
        //   3. do sub, return sub
        let sub = subsystem.subscribe::<MyVersionMessage>().await.unwrap();

        let msg = MyVersionMessage { x: 110 };
        let mut payload = Vec::new();
        msg.encode(&mut payload).unwrap();

        // receive message and publish
        //   1. based on string, lookup relevant dispatcher interface
        //   2. publish data there
        subsystem.notify("verver", payload).await;

        // receive
        //    1. do a get easy
        let msg2 = sub.receive().await.unwrap();
        assert_eq!(msg2.x, 110);
        println!("{}", msg2.x);

        subsystem.trigger_error(Error::ChannelStopped).await;

        let msg2 = sub.receive().await;
        assert!(msg2.is_err());

        sub.unsubscribe().await;
    }
}
