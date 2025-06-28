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

use std::{collections::HashMap, sync::Arc};

use log::warn;
use rand::{rngs::OsRng, Rng};
use smol::lock::Mutex;

pub type PublisherPtr<T> = Arc<Publisher<T>>;
pub type SubscriptionId = usize;

#[derive(Debug)]
/// Subscription to the Publisher. Created using `publisher.subscribe().await`.
pub struct Subscription<T> {
    id: SubscriptionId,
    recv_queue: smol::channel::Receiver<T>,
    parent: Arc<Publisher<T>>,
}

impl<T: Clone> Subscription<T> {
    pub fn get_id(&self) -> SubscriptionId {
        self.id
    }

    /// Receive message.
    pub async fn receive(&self) -> T {
        let message_result = self.recv_queue.recv().await;

        match message_result {
            Ok(message_result) => message_result,
            Err(err) => {
                panic!("Subscription::receive() recv_queue failed! {err}");
            }
        }
    }

    /// Must be called manually since async Drop is not possible in Rust
    pub async fn unsubscribe(&self) {
        self.parent.clone().unsubscribe(self.id).await
    }
}

/// Simple broadcast (publish-subscribe) class.
#[derive(Debug)]
pub struct Publisher<T> {
    subs: Mutex<HashMap<SubscriptionId, smol::channel::Sender<T>>>,
}

impl<T: Clone> Publisher<T> {
    /// Construct a new publisher.
    pub fn new() -> Arc<Self> {
        Arc::new(Self { subs: Mutex::new(HashMap::new()) })
    }

    fn random_id() -> SubscriptionId {
        OsRng.gen()
    }

    /// Make sure you call this method early in your setup. That way the subscription
    /// will begin accumulating messages from notify.
    /// Then when your main loop begins calling `sub.receive().await`, the messages will
    /// already be queued.
    pub async fn subscribe(self: Arc<Self>) -> Subscription<T> {
        let (sender, recvr) = smol::channel::unbounded();

        // Poor-man's do/while
        let mut subs = self.subs.lock().await;
        let mut sub_id = Self::random_id();
        while subs.contains_key(&sub_id) {
            sub_id = Self::random_id();
        }

        subs.insert(sub_id, sender);

        Subscription { id: sub_id, recv_queue: recvr, parent: self.clone() }
    }

    async fn unsubscribe(self: Arc<Self>, sub_id: SubscriptionId) {
        self.subs.lock().await.remove(&sub_id);
    }

    /// Publish a message to all listening subscriptions.
    pub async fn notify(&self, message_result: T) {
        self.notify_with_exclude(message_result, &[]).await
    }

    /// Publish a message to all listening subscriptions but exclude some subset.
    pub async fn notify_with_exclude(&self, message_result: T, exclude_list: &[SubscriptionId]) {
        for (id, sub) in (*self.subs.lock().await).iter() {
            if exclude_list.contains(id) {
                continue
            }

            if let Err(e) = sub.send(message_result.clone()).await {
                warn!(
                    target: "system::publisher",
                    "[system::publisher] Error returned sending message in notify_with_exclude() call! {e}"
                );
            }
        }
    }
}
