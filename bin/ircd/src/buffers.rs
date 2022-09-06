use async_std::sync::{Arc, Mutex};
use std::{cmp::Ordering, collections::VecDeque};

use chrono::Utc;
use fxhash::FxHashMap;
use ripemd::{Digest, Ripemd160};

use crate::Privmsg;

pub const SIZE_OF_MSGS_BUFFER: usize = 4095;
pub const SIZE_OF_MSG_IDSS_BUFFER: usize = 65536;
pub const LIFETIME_FOR_ORPHAN: i64 = 600;

pub type InvSeenIds = Arc<Mutex<RingBuffer<u64>>>;
pub type SeenIds = Mutex<RingBuffer<u64>>;
pub type MutexPrivmsgsBuffer = Mutex<PrivmsgsBuffer>;
pub type UnreadMsgs = Mutex<UMsgs>;
pub type Buffers = Arc<Msgs>;

pub struct Msgs {
    pub privmsgs: MutexPrivmsgsBuffer,
    pub unread_msgs: UnreadMsgs,
    pub seen_ids: SeenIds,
}

pub fn create_buffers() -> Buffers {
    let seen_ids = Mutex::new(RingBuffer::new(SIZE_OF_MSG_IDSS_BUFFER));
    let privmsgs = PrivmsgsBuffer::new();
    let unread_msgs = Mutex::new(UMsgs::new());
    Arc::new(Msgs { privmsgs, unread_msgs, seen_ids })
}

#[derive(Clone)]
pub struct UMsgs(pub FxHashMap<String, Privmsg>);

impl UMsgs {
    pub fn new() -> Self {
        Self(FxHashMap::default())
    }

    pub fn insert(&mut self, msg: &Privmsg) -> String {
        let mut hasher = Ripemd160::new();
        hasher.update(msg.to_string());
        let key = hex::encode(hasher.finalize());
        self.0.insert(key.clone(), msg.clone());
        key
    }
}

impl Default for UMsgs {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
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

#[derive(Clone)]
pub struct PrivmsgsBuffer {
    buffer: RingBuffer<Privmsg>,
    orphans: RingBuffer<Orphan>,
}

impl PrivmsgsBuffer {
    pub fn new() -> MutexPrivmsgsBuffer {
        Mutex::new(Self {
            buffer: RingBuffer::new(SIZE_OF_MSGS_BUFFER),
            orphans: RingBuffer::new(SIZE_OF_MSGS_BUFFER),
        })
    }

    pub fn push(&mut self, privmsg: &Privmsg) {
        match privmsg.term.cmp(&(self.last_term() + 1)) {
            Ordering::Equal | Ordering::Less => self.buffer.push(privmsg.clone()),
            Ordering::Greater => self.orphans.push(Orphan::new(privmsg)),
        }
        self.update();
    }

    pub fn iter(&self) -> impl Iterator<Item = &Privmsg> + DoubleEndedIterator {
        self.buffer.iter()
    }

    pub fn last_term(&self) -> u64 {
        match self.buffer.len() {
            0 => 0,
            n => self.buffer.items[n - 1].term,
        }
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

    fn oprhan_is_valid(&mut self, orphan: &Orphan) -> bool {
        (orphan.timestamp + LIFETIME_FOR_ORPHAN) > Utc::now().timestamp()
    }

    fn update_orphans(&mut self) {
        for orphan in self.orphans.clone().iter() {
            let privmsg = orphan.msg.clone();

            if !self.oprhan_is_valid(orphan) {
                self.orphans.remove(orphan);
                continue
            }

            match privmsg.term.cmp(&(self.last_term() + 1)) {
                Ordering::Equal | Ordering::Less => {
                    self.buffer.push(privmsg.clone());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Privmsg;
    use rand::{seq::SliceRandom, thread_rng};

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

    #[test]
    fn test_privmsgs_buffer() {
        let mut pms = PrivmsgsBuffer {
            buffer: RingBuffer::new(SIZE_OF_MSGS_BUFFER),
            orphans: RingBuffer::new(SIZE_OF_MSGS_BUFFER),
        };

        //
        // Fill the buffer with random generated terms in range 0..3001
        //
        let mut terms: Vec<u64> = (1..3001).collect();
        terms.shuffle(&mut thread_rng());

        for term in terms {
            let privmsg = Privmsg::new("nick", "#dev", &format!("message_{}", term), term);
            pms.push(&privmsg);
        }

        assert_eq!(pms.buffer.len(), 3000);
        assert_eq!(pms.last_term(), 3000);
        assert_eq!(pms.orphans.len(), 0);

        //
        // Fill the buffer with random generated terms in range 2000..4001
        // Since the buffer len now is 3000 it will take only the terms from
        // 3001 to 4000 without overwriting
        //
        let mut terms: Vec<u64> = (2000..4001).collect();
        terms.shuffle(&mut thread_rng());

        for term in terms {
            let privmsg = Privmsg::new("nick", "#dev", &format!("message_{}", term), term);
            pms.push(&privmsg);
        }

        assert_eq!(pms.buffer.len(), SIZE_OF_MSGS_BUFFER);
        assert_eq!(pms.last_term(), 4000);
        assert_eq!(pms.orphans.len(), 0);

        //
        // Fill the buffer with random generated terms in range 4000..7001
        // Since the buffer max size is SIZE_OF_MSGS_BUFFER it has to remove the old msges
        //
        let mut terms: Vec<u64> = (4001..7001).collect();
        terms.shuffle(&mut thread_rng());

        for term in terms {
            let privmsg = Privmsg::new("nick", "#dev", &format!("message_{}", term), term);
            pms.push(&privmsg);
        }

        assert_eq!(pms.buffer.len(), SIZE_OF_MSGS_BUFFER);
        assert_eq!(pms.last_term(), 7000);
        assert_eq!(pms.orphans.len(), 0);
    }
}
