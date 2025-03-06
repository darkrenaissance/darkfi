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

use std::{any::Any, collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use log::{debug, error};
use rand::{rngs::OsRng, Rng};
use smol::{io::AsyncReadExt, lock::Mutex};

use super::message::Message;
use crate::{
    net::{metering::MeteringQueue, transport::PtStream},
    system::{msleep, timeout::timeout},
    Error, Result,
};
use darkfi_serial::{AsyncDecodable, VarInt};

/// 64-bit identifier for message subscription.
pub type MessageSubscriptionId = u64;
type MessageResult<M> = Result<Arc<M>>;

/// Dispatcher subscriptions HashMap type.
type DispatcherSubscriptionsMap<M> =
    Mutex<HashMap<MessageSubscriptionId, smol::channel::Sender<(MessageResult<M>, Option<u64>)>>>;

/// A dispatcher that is unique to every [`Message`].
///
/// Maintains a list of subscriptions to a unique Message
/// type and handles sending messages across these
/// subscriptions.
///
/// Additionally, holds a `MeteringQueue` using the
/// [`Message`] configuration to perform rate limiting
/// of propagation towards the subscriptions.
#[derive(Debug)]
struct MessageDispatcher<M: Message> {
    subs: DispatcherSubscriptionsMap<M>,
    metering_queue: Mutex<MeteringQueue>,
}

impl<M: Message> MessageDispatcher<M> {
    /// Create a new message dispatcher
    fn new() -> Self {
        Self {
            subs: Mutex::new(HashMap::new()),
            metering_queue: Mutex::new(MeteringQueue::new(M::METERING_CONFIGURATION)),
        }
    }

    /// Create a random ID.
    fn random_id() -> MessageSubscriptionId {
        OsRng.gen()
    }

    /// Subscribe to a channel.
    /// Assigns a new ID and adds it to the list of subscriptions.
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

        let msg_result_type = if message.is_ok() { "Ok" } else { "Err" };
        debug!(
            target: "net::message_publisher::_trigger_all()", "START msg={}({}), subs={}",
            msg_result_type,
            M::NAME, subs.len(),
        );

        // Insert metering information and grab potential sleep time
        let mut queue = self.metering_queue.lock().await;
        queue.push(&M::METERING_SCORE);
        let sleep_time = queue.sleep_time();
        drop(queue);

        let mut futures = FuturesUnordered::new();
        let mut garbage_ids = vec![];

        // Prep the futures for concurrent execution
        for (sub_id, sub) in &*subs {
            let sub_id = *sub_id;
            let sub = sub.clone();
            let message = message.clone();
            futures.push(async move {
                match sub.send((message, sleep_time)).await {
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
            target: "net::message_publisher::_trigger_all()", "END msg={}({}), subs={}",
            msg_result_type,
            M::NAME, subs.len(),
        );
    }
}

/// Handles message subscriptions through a subscription ID and
/// a receiver channel.
#[derive(Debug)]
pub struct MessageSubscription<M: Message> {
    id: MessageSubscriptionId,
    recv_queue: smol::channel::Receiver<(MessageResult<M>, Option<u64>)>,
    parent: Arc<MessageDispatcher<M>>,
}

impl<M: Message> MessageSubscription<M> {
    /// Start receiving messages.
    /// Sender also provides with a sleep time,
    /// in case rate limit has started.
    pub async fn receive(&self) -> MessageResult<M> {
        let (message, sleep_time) = match self.recv_queue.recv().await {
            Ok(pair) => pair,
            Err(e) => panic!("MessageSubscription::receive(): recv_queue failed! {}", e),
        };

        // Check if we need to sleep
        if message.is_ok() {
            if let Some(sleep_time) = sleep_time {
                msleep(sleep_time).await;
            }
        }

        message
    }

    /// Start receiving messages with timeout.
    pub async fn receive_with_timeout(&self, seconds: u64) -> MessageResult<M> {
        let dur = Duration::from_secs(seconds);
        let Ok(res) = timeout(dur, self.recv_queue.recv()).await else {
            return Err(Error::ConnectTimeout)
        };

        let (message, sleep_time) = match res {
            Ok(pair) => pair,
            Err(e) => {
                panic!("MessageSubscription::receive_with_timeout(): recv_queue failed! {}", e)
            }
        };

        // Check if we need to sleep
        if message.is_ok() {
            if let Some(sleep_time) = sleep_time {
                msleep(sleep_time).await;
            }
        }

        message
    }

    /// Cleans existing items from the receiver channel.
    pub async fn clean(&self) -> Result<()> {
        loop {
            match self.recv_queue.try_recv() {
                Ok(_) => continue,
                Err(smol::channel::TryRecvError::Empty) => return Ok(()),
                Err(e) => panic!("MessageSubscription::receive(): recv_queue failed! {}", e),
            }
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
    async fn trigger(
        &self,
        stream: &mut smol::io::ReadHalf<Box<dyn PtStream + 'static>>,
    ) -> Result<()>;

    async fn trigger_error(&self, err: Error);

    async fn metering_score(&self) -> u64;

    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

/// Local implementation of the Message Dispatcher Interface
#[async_trait]
impl<M: Message> MessageDispatcherInterface for MessageDispatcher<M> {
    /// Internal function to deserialize data into a message type
    /// and dispatch it across subscriber channels. Reads directly
    /// from an inbound stream.
    ///
    /// We extract the message length from the stream and use `take()`
    /// to allocate an appropiately sized buffer as a basic DDOS protection.
    async fn trigger(
        &self,
        stream: &mut smol::io::ReadHalf<Box<dyn PtStream + 'static>>,
    ) -> Result<()> {
        // Parse message length
        let length = match VarInt::decode_async(stream).await {
            Ok(int) => int.0,
            Err(err) => {
                error!(
                    target: "net::message_publisher::trigger()",
                    "Unable to decode VarInt. Dropping...: {}",
                    err,
                );
                return Err(Error::MessageInvalid)
            }
        };

        // Check the message length does not exceed set limit
        if M::MAX_BYTES > 0 && length > M::MAX_BYTES {
            error!(
                target: "net::message_publisher::trigger()",
                "Message length ({}) exceeds configured limit ({}). Dropping...",
                length, M::MAX_BYTES,
            );
            return Err(Error::MessageInvalid)
        }

        // Deserialize stream into type
        let mut take = stream.take(length);
        let message = match M::decode_async(&mut take).await {
            Ok(payload) => Ok(Arc::new(payload)),
            Err(err) => {
                error!(
                    target: "net::message_publisher::trigger()",
                    "Unable to decode data. Dropping...: {}",
                    err,
                );
                return Err(Error::MessageInvalid)
            }
        };

        // Send down the pipes
        self._trigger_all(message).await;
        Ok(())
    }

    /// Internal function that sends an error message to all subscriber channels.
    async fn trigger_error(&self, err: Error) {
        self._trigger_all(Err(err)).await;
    }

    /// Internal function to retrieve metering queue current total score,
    /// after prunning expired metering information.
    async fn metering_score(&self) -> u64 {
        let mut lock = self.metering_queue.lock().await;
        lock.clean();
        lock.total()
    }

    /// Converts to `Any` trait. Enables the dynamic modification of static types.
    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}

/// Generic publish/subscribe class that maintains a list of dispatchers.
///
/// Dispatchers transmit messages to subscribers and are specific to one
/// message type.
///
/// Additionally, holds a global metering limit, which is the sum of each
/// dispatcher `MeteringQueue` threshold, to drop the connection if passed.
#[derive(Default)]
pub struct MessageSubsystem {
    dispatchers: Mutex<HashMap<&'static str, Arc<dyn MessageDispatcherInterface>>>,
    metering_limit: Mutex<u64>,
}

impl MessageSubsystem {
    /// Create a new message subsystem.
    pub fn new() -> Self {
        Self { dispatchers: Mutex::new(HashMap::new()), metering_limit: Mutex::new(0) }
    }

    /// Add a new dispatcher for specified [`Message`].
    pub async fn add_dispatch<M: Message>(&self) {
        // First lock the dispatchers
        let mut lock = self.dispatchers.lock().await;

        // Update the metering limit
        *self.metering_limit.lock().await += M::METERING_CONFIGURATION.threshold;

        // Insert the new dispatcher
        lock.insert(M::NAME, Arc::new(MessageDispatcher::<M>::new()));
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
    pub async fn notify(
        &self,
        command: &str,
        reader: &mut smol::io::ReadHalf<Box<dyn PtStream + 'static>>,
    ) -> Result<()> {
        // Iterate over dispatchers and keep track of their current
        // metering score
        let mut found = false;
        let mut total_score = 0;
        for (name, dispatcher) in self.dispatchers.lock().await.iter() {
            // If dispatcher is the command one, trasmit the message
            if name == &command {
                dispatcher.trigger(reader).await?;
                found = true;
            }

            // Grab its total score
            total_score += dispatcher.metering_score().await;
        }

        // Check if dispatcher was found
        if !found {
            return Err(Error::MissingDispatcher)
        }

        // Check if we are over the global metering limit
        if total_score > *self.metering_limit.lock().await {
            return Err(Error::MeteringLimitExceeded)
        }

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
