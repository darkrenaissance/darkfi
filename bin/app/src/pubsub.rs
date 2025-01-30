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

use rand::{rngs::OsRng, Rng};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::error::{Error, Result};

pub type SubscriptionId = usize;

// Waiting for trait aliases
pub trait Piped: Clone + Send + 'static {}
impl<T> Piped for T where T: Clone + Send + 'static {}

#[derive(Debug)]
/// Subscription to the Publisher. Created using `publisher.subscribe().await`.
pub struct Subscription<T: Piped> {
    id: SubscriptionId,
    recv_queue: smol::channel::Receiver<T>,
    parent: Arc<Publisher<T>>,
}

impl<T: Piped> Subscription<T> {
    pub fn get_id(&self) -> SubscriptionId {
        self.id
    }

    /// Receive message.
    pub async fn receive(&self) -> Result<T> {
        let msg_result = self.recv_queue.recv().await;
        msg_result.or(Err(Error::PublisherDestroyed))
    }
}

impl<T: Piped> Drop for Subscription<T> {
    fn drop(&mut self) {
        self.parent.unsubscribe(self.id)
    }
}

pub type PublisherPtr<T> = Arc<Publisher<T>>;

#[derive(Debug)]
pub struct Publisher<T> {
    subs: Mutex<HashMap<SubscriptionId, smol::channel::Sender<T>>>,
}

impl<T: Piped> Publisher<T> {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { subs: Mutex::new(HashMap::new()) })
    }

    pub fn subscribe(self: Arc<Self>) -> Subscription<T> {
        let (sendr, recvr) = smol::channel::unbounded();
        let sub_id = OsRng.gen();
        // Optional to check whether this ID already exists.
        // It is nearly impossible to ever happen.
        self.subs.lock().unwrap().insert(sub_id, sendr);

        Subscription { id: sub_id, recv_queue: recvr, parent: self.clone() }
    }

    fn unsubscribe(&self, sub_id: SubscriptionId) {
        self.subs.lock().unwrap().remove(&sub_id);
    }

    /// Publish a message to subscriptions in the include list
    pub fn notify_with_include(&self, message_result: T, include_list: &[SubscriptionId]) {
        // Maybe we should just provide a method to get all IDs
        // Then people can call notify_with_exclude() instead.
        // TODO: just collect and clone directly into a Vec
        let subs = self.subs.lock().unwrap().clone();
        for (id, sub) in subs.into_iter() {
            if !include_list.contains(&id) {
                continue
            }

            if let Err(e) = sub.try_send(message_result.clone()) {
                panic!("[system::publisher] Error returned sending message in notify_with_include() call! {}", e);
            }
        }
    }

    /// Publish a message to all listening subscriptions.
    pub fn notify(&self, msg: T) {
        let subs = self.subs.lock().unwrap().clone();
        for (id, sub) in subs {
            if let Err(e) = sub.try_send(msg.clone()) {
                // This should never happen since Drop calls unsubscribe()
                panic!("Error in notify() call for sub={}! {}", id, e);
            }
        }
    }
}
