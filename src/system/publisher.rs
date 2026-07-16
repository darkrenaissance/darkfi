/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use parking_lot::Mutex;
use rand::{rngs::OsRng, Rng};

pub type PublisherPtr<T> = Arc<Publisher<T>>;
pub type SubscriptionId = usize;

/// Maximum number of pending notifications retained by each subscription.
/// When full, the oldest notification is discarded so producers never block
/// and the subscriber receives the newest state or shutdown signal.
pub const PUBLISHER_QUEUE_CAPACITY: usize = 1024;

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

    /// Remove this subscription immediately. Dropping it has the same effect.
    pub async fn unsubscribe(&self) {
        self.parent.unsubscribe(self.id)
    }
}

impl<T> Drop for Subscription<T> {
    fn drop(&mut self) {
        self.parent.unsubscribe(self.id)
    }
}

/// Simple broadcast (publish-subscribe) class.
#[derive(Debug)]
pub struct Publisher<T> {
    subs: Mutex<HashMap<SubscriptionId, smol::channel::Sender<T>>>,
    queue_capacity: usize,
    dropped_notifications: AtomicUsize,
}

impl<T> Publisher<T> {
    /// Construct a new publisher.
    pub fn new() -> Arc<Self> {
        Self::with_capacity(PUBLISHER_QUEUE_CAPACITY)
    }

    /// Construct a publisher with a custom per-subscription queue capacity.
    pub fn with_capacity(queue_capacity: usize) -> Arc<Self> {
        assert!(queue_capacity > 0, "publisher queue capacity must be nonzero");
        Arc::new(Self {
            subs: Mutex::new(HashMap::new()),
            queue_capacity,
            dropped_notifications: AtomicUsize::new(0),
        })
    }

    fn random_id() -> SubscriptionId {
        OsRng.gen()
    }

    /// Make sure you call this method early in your setup. That way the subscription
    /// will begin accumulating messages from notify.
    /// Then when your main loop begins calling `sub.receive().await`, the messages will
    /// already be queued.
    pub async fn subscribe(self: Arc<Self>) -> Subscription<T> {
        let (sender, recvr) = smol::channel::bounded(self.queue_capacity);

        // Poor-man's do/while
        let mut subs = self.subs.lock();
        let mut sub_id = Self::random_id();
        while subs.contains_key(&sub_id) {
            sub_id = Self::random_id();
        }

        subs.insert(sub_id, sender);
        drop(subs);

        Subscription { id: sub_id, recv_queue: recvr, parent: self }
    }

    fn unsubscribe(&self, sub_id: SubscriptionId) {
        self.subs.lock().remove(&sub_id);
    }

    /// Number of oldest notifications discarded due to full subscriber queues.
    pub fn dropped_notifications(&self) -> usize {
        self.dropped_notifications.load(Ordering::Relaxed)
    }

    /// Number of currently registered subscriptions.
    pub fn active_subscriptions(&self) -> usize {
        self.subs.lock().len()
    }
}

impl<T: Clone> Publisher<T> {
    /// Publish a message to all listening subscriptions.
    pub async fn notify(&self, message_result: T) {
        self.notify_with_exclude(message_result, &[]).await
    }

    /// Publish a message to all listening subscriptions but exclude some subset.
    pub async fn notify_with_exclude(&self, message_result: T, exclude_list: &[SubscriptionId]) {
        let mut overflowed = 0;
        self.subs.lock().retain(|id, sub| {
            if exclude_list.contains(id) {
                return true
            }

            match sub.force_send(message_result.clone()) {
                Ok(Some(_)) => {
                    overflowed += 1;
                    true
                }
                Ok(None) => true,
                Err(_) => false,
            }
        });

        if overflowed > 0 {
            self.dropped_notifications.fetch_add(overflowed, Ordering::Relaxed);
        }
    }

    /// Clear inactive subscriptions.
    /// Returns `true` when no active subscriptions remain after cleanup.
    pub async fn clear_inactive(&self) -> bool {
        // Grab a lock over current jobs
        let mut subs = self.subs.lock();

        // Find inactive subscriptions
        let mut dropped = vec![];
        for (sub, channel) in subs.iter() {
            if channel.receiver_count() == 0 {
                dropped.push(*sub);
            }
        }

        // Drop inactive subscriptions
        for sub in dropped {
            subs.remove(&sub);
        }

        // Return whether no subscriptions remain.
        subs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_subscription_removes_sender() {
        smol::block_on(async {
            let publisher = Publisher::<u32>::new();
            let subscription = publisher.clone().subscribe().await;
            assert_eq!(publisher.subs.lock().len(), 1);

            drop(subscription);
            assert!(publisher.subs.lock().is_empty());
        });
    }

    #[test]
    fn slow_subscription_keeps_newest_notifications() {
        smol::block_on(async {
            let publisher = Publisher::with_capacity(2);
            let subscription = publisher.clone().subscribe().await;

            publisher.notify(1).await;
            publisher.notify(2).await;
            publisher.notify(3).await;

            assert_eq!(subscription.receive().await, 2);
            assert_eq!(subscription.receive().await, 3);
            assert_eq!(publisher.dropped_notifications(), 1);
        });
    }

    #[test]
    fn explicit_unsubscribe_removes_sender() {
        smol::block_on(async {
            let publisher = Publisher::<u32>::new();
            let subscription = publisher.clone().subscribe().await;

            subscription.unsubscribe().await;
            assert!(publisher.subs.lock().is_empty());
        });
    }

    #[test]
    fn notify_removes_closed_sender() {
        smol::block_on(async {
            let publisher = Publisher::<u32>::new();
            let (sender, receiver) = smol::channel::bounded(1);
            drop(receiver);
            publisher.subs.lock().insert(1, sender);

            publisher.notify(1).await;
            assert!(publisher.subs.lock().is_empty());
        });
    }
}
