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

use async_std::sync::{Arc, Mutex};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, VecDeque},
};

use chrono::Utc;
use ripemd::{Digest, Ripemd160};

use crate::{settings, Privmsg};

pub type Buffers = Arc<Msgs>;

pub struct Msgs {
    pub privmsgs: PrivmsgsBuffer,
    pub unread_msgs: UMsgs,
    pub seen_ids: SeenIds,
}

pub fn create_buffers() -> Buffers {
    let seen_ids = SeenIds::new();
    let privmsgs = PrivmsgsBuffer::new();
    let unread_msgs = UMsgs::new();
    Arc::new(Msgs { privmsgs, unread_msgs, seen_ids })
}

#[derive(Default, Clone)]
pub struct RingBuffer<T> {
    pub items: VecDeque<T>,
}

impl<T: Eq + PartialEq + Clone> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        let items = VecDeque::with_capacity(capacity);
        Self { items }
    }

    pub fn push(&mut self, val: T) {
        if self.items.len() == self.items.capacity() {
            self.items.pop_front();
        }
        self.items.push_back(val);
    }

    pub fn contains(&self, val: &T) -> bool {
        self.items.contains(val)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn as_slice(&mut self) -> &mut [T] {
        self.items.make_contiguous()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> + DoubleEndedIterator {
        self.items.iter()
    }

    pub fn remove(&mut self, val: &T) -> Option<T> {
        if let Some(index) = self.items.iter().position(|v| v == val) {
            self.items.remove(index)
        } else {
            None
        }
    }
}

#[derive(Default)]
pub struct PrivmsgsBuffer {
    msgs: Mutex<OrderingAlgo>,
}

impl PrivmsgsBuffer {
    pub fn new() -> Self {
        Self { msgs: Mutex::new(OrderingAlgo::new()) }
    }

    pub async fn push(&self, privmsg: &Privmsg) {
        self.msgs.lock().await.push(privmsg);
    }

    pub async fn load(&self) -> Vec<Privmsg> {
        self.msgs.lock().await.load()
    }

    pub async fn get_msg_by_term(&self, term: u64) -> Option<Privmsg> {
        self.msgs.lock().await.get_msg_by_term(term)
    }

    pub async fn len(&self) -> usize {
        self.msgs.lock().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.msgs.lock().await.is_empty()
    }

    pub async fn last_term(&self) -> u64 {
        self.msgs.lock().await.last_term()
    }

    pub async fn fetch_msgs(&self, term: u64) -> Vec<Privmsg> {
        self.msgs.lock().await.fetch_msgs(term)
    }
}

pub struct OrderingAlgo {
    buffer: RingBuffer<Privmsg>,
    orphans: RingBuffer<Orphan>,
}

impl Default for OrderingAlgo {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderingAlgo {
    pub fn new() -> Self {
        Self {
            buffer: RingBuffer::new(settings::SIZE_OF_MSGS_BUFFER),
            orphans: RingBuffer::new(settings::SIZE_OF_MSGS_BUFFER),
        }
    }

    pub fn push(&mut self, privmsg: &Privmsg) {
        match privmsg.term.cmp(&(self.last_term() + 1)) {
            Ordering::Equal => self.buffer.push(privmsg.clone()),
            Ordering::Less => {
                if let Some(msg) = self.get_msg_by_term(privmsg.term) {
                    if (msg.timestamp - privmsg.timestamp) <= settings::TERM_MAX_TIME_DIFFERENCE {
                        self.buffer.push(privmsg.clone());
                    }
                } else {
                    self.buffer.push(privmsg.clone());
                }
            }
            Ordering::Greater => self.orphans.push(Orphan::new(privmsg)),
        }
        self.update();
    }

    pub fn load(&self) -> Vec<Privmsg> {
        self.buffer.iter().cloned().collect::<Vec<Privmsg>>()
    }

    pub fn get_msg_by_term(&self, term: u64) -> Option<Privmsg> {
        self.buffer.iter().find(|p| p.term == term).cloned()
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn last_term(&self) -> u64 {
        match self.buffer.len() {
            0 => 0,
            n => self.buffer.items[n - 1].term,
        }
    }

    pub fn fetch_msgs(&self, term: u64) -> Vec<Privmsg> {
        self.buffer.iter().take_while(|p| p.term >= term).cloned().collect()
    }

    fn update(&mut self) {
        self.sort_orphans();
        self.update_orphans();
        self.sort_buffer();
    }

    fn sort_buffer(&mut self) {
        self.buffer.as_slice().sort_by(|a, b| match a.term.cmp(&b.term) {
            Ordering::Equal => a.timestamp.cmp(&b.timestamp),
            o => o,
        });
    }

    fn sort_orphans(&mut self) {
        self.orphans.as_slice().sort_by(|a, b| match a.msg.term.cmp(&b.msg.term) {
            Ordering::Equal => a.msg.timestamp.cmp(&b.msg.timestamp),
            o => o,
        });
    }

    fn oprhan_is_valid(orphan: &Orphan) -> bool {
        (orphan.timestamp + settings::LIFETIME_FOR_ORPHAN) > Utc::now().timestamp()
    }

    fn update_orphans(&mut self) {
        for orphan in self.orphans.clone().iter() {
            let privmsg = orphan.msg.clone();

            if !Self::oprhan_is_valid(orphan) {
                self.orphans.remove(orphan);
                continue
            }

            match privmsg.term.cmp(&(self.last_term() + 1)) {
                Ordering::Equal => {
                    self.buffer.push(privmsg.clone());
                    self.orphans.remove(orphan);
                }
                Ordering::Less => {
                    if let Some(msg) = self.get_msg_by_term(privmsg.term) {
                        if (msg.timestamp - privmsg.timestamp) <= settings::TERM_MAX_TIME_DIFFERENCE
                        {
                            self.buffer.push(privmsg.clone());
                        }
                    } else {
                        self.buffer.push(privmsg.clone());
                    }
                    self.orphans.remove(orphan);
                }
                Ordering::Greater => {}
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct Orphan {
    msg: Privmsg,
    timestamp: i64,
}

impl Orphan {
    fn new(privmsg: &Privmsg) -> Self {
        Self { msg: privmsg.clone(), timestamp: Utc::now().timestamp() }
    }
}

pub struct SeenIds {
    ids: RingBuffer<u64>,
}

impl Default for SeenIds {
    fn default() -> Self {
        Self::new()
    }
}

impl SeenIds {
    pub fn new() -> Self {
        Self { ids: RingBuffer::new(settings::SIZE_OF_IDSS_BUFFER) }
    }

    pub fn push(&mut self, id: u64) -> bool {
        if !self.ids.contains(&id) {
            self.ids.push(id);
            return true
        }
        false
    }
}

pub struct UMsgs {
    msgs: Mutex<BTreeMap<String, Privmsg>>,
}

impl Default for UMsgs {
    fn default() -> Self {
        Self::new()
    }
}

impl UMsgs {
    pub fn new() -> Self {
        Self { msgs: Mutex::new(BTreeMap::new()) }
    }

    pub async fn len(&self) -> usize {
        self.msgs.lock().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.msgs.lock().await.len() == 0
    }

    pub async fn contains(&self, key: &str) -> bool {
        self.msgs.lock().await.contains_key(key)
    }

    pub async fn remove(&self, key: &str) -> Option<Privmsg> {
        self.msgs.lock().await.remove(key)
    }

    pub async fn get(&self, key: &str) -> Option<Privmsg> {
        self.msgs.lock().await.get(key).cloned()
    }

    pub async fn load(&self) -> BTreeMap<String, Privmsg> {
        self.msgs.lock().await.clone()
    }

    pub async fn inc_read_confirms(&self, key: &str) -> bool {
        if let Some(msg) = self.msgs.lock().await.get_mut(key) {
            msg.read_confirms += 1;
            return true
        }
        false
    }

    pub async fn insert(&self, msg: &Privmsg) -> String {
        let mut hasher = Ripemd160::new();
        hasher.update(msg.to_string() + &msg.term.to_string() + &msg.timestamp.to_string());
        let key = hex::encode(hasher.finalize());

        let msgs = &mut self.msgs.lock().await;
        if msgs.len() == settings::SIZE_OF_MSGS_BUFFER {
            let first_key = msgs.iter().next_back().unwrap().0.clone();
            msgs.remove(&first_key);
        }

        msgs.insert(key.clone(), msg.clone());
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Privmsg;

    #[test]
    fn test_ring_buffer() {
        let mut b = RingBuffer::<&str>::new(3);
        b.push("h1");
        b.push("h2");
        b.push("h3");
        assert_eq!(b.items, vec!["h1", "h2", "h3"]);
        assert_eq!(b.items.capacity(), 3);
        b.push("h4");
        assert_eq!(b.items, vec!["h2", "h3", "h4"]);
        assert_eq!(b.items.capacity(), 3);
        b.push("h5");
        b.push("h6");
        b.push("h7");
        b.push("h8");
        b.push("h9");
        assert_eq!(b.len(), 3);
        assert_eq!(b.iter().last().unwrap(), &"h9");
    }

    #[async_std::test]
    async fn test_unread_msgs() {
        let unread_msgs = UMsgs::default();

        let p = Privmsg::new("nick", "#dev", &format!("message_{}", 0), 0);
        let p_k = unread_msgs.insert(&p).await;

        let p2 = Privmsg::new("nick", "#dev", &format!("message_{}", 0), 1);
        let p2_k = unread_msgs.insert(&p2).await;

        let p3 = Privmsg::new("nick", "#dev", &format!("message_{}", 0), 2);
        let p3_k = unread_msgs.insert(&p3).await;

        assert_eq!(unread_msgs.len().await, 3);

        assert_eq!(unread_msgs.get(&p_k).await, Some(p.clone()));
        assert_eq!(unread_msgs.get(&p2_k).await, Some(p2));
        assert_eq!(unread_msgs.get(&p3_k).await, Some(p3));

        assert!(unread_msgs.inc_read_confirms(&p_k).await);
        assert!(!unread_msgs.inc_read_confirms("NONE KEY").await);

        assert_ne!(unread_msgs.get(&p_k).await, Some(p));
        assert_eq!(unread_msgs.get(&p_k).await.unwrap().read_confirms, 1);
    }
}
