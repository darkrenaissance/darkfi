use async_std::sync::{Arc, Mutex};
use std::{cmp::Ordering, collections::VecDeque};

use chrono::Utc;

use crate::Privmsg;

pub const SIZE_OF_MSGS_BUFFER: usize = 4095;
pub const SIZE_OF_MSG_IDSS_BUFFER: usize = 65536;
pub const LIFETIME_FOR_ORPHAN: i64 = 600;

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

    pub fn iter(&self) -> impl Iterator<Item = &T> {
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

pub type SeenMsgIds = Arc<Mutex<RingBuffer<u64>>>;

pub type ArcPrivmsgsBuffer = Arc<Mutex<PrivmsgsBuffer>>;

pub struct PrivmsgsBuffer {
    buffer: RingBuffer<Privmsg>,
    orphans: RingBuffer<Orphan>,
}

impl PrivmsgsBuffer {
    pub fn new() -> ArcPrivmsgsBuffer {
        Arc::new(Mutex::new(Self {
            buffer: RingBuffer::new(SIZE_OF_MSGS_BUFFER),
            orphans: RingBuffer::new(SIZE_OF_MSGS_BUFFER),
        }))
    }

    pub fn push(&mut self, privmsg: &Privmsg) {
        if self.buffer.contains(privmsg) {
            return
        }

        match privmsg.term.cmp(&(self.last_term() + 1)) {
            Ordering::Equal => self.buffer.push(privmsg.clone()),
            Ordering::Greater => self.orphans.push(Orphan::new(privmsg)),
            Ordering::Less => {
                if !self.term_exist(privmsg.term) {
                    self.orphans.push(Orphan::new(privmsg))
                }
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Privmsg> {
        self.buffer.iter()
    }

    pub fn last_term(&self) -> u64 {
        match self.buffer.len() {
            0 => 0,
            n => self.buffer.items[n - 1].term,
        }
    }

    pub fn update(&mut self) {
        self.sort_orphans();
        self.push_orphans();
        self.sort();
    }

    fn term_exist(&self, term: u64) -> bool {
        self.buffer.items.iter().any(|p| p.term == term)
    }

    fn sort(&mut self) {
        self.buffer.as_slice().sort_by(|a, b| a.term.cmp(&b.term));
    }

    fn sort_orphans(&mut self) {
        self.orphans.as_slice().sort_by(|a, b| a.msg.term.cmp(&b.msg.term));
    }

    fn push_orphans(&mut self) {
        for orphan in self.orphans.clone().iter() {
            let privmsg = orphan.msg.clone();
            match privmsg.term.cmp(&(self.last_term() + 1)) {
                Ordering::Equal => {
                    self.buffer.push(privmsg.clone());
                    self.orphans.remove(orphan);
                }
                Ordering::Less => {
                    if !self.term_exist(privmsg.term) {
                        self.buffer.push(privmsg.clone());
                    }
                    self.orphans.remove(orphan);
                }
                Ordering::Greater => {
                    if (orphan.timestamp + LIFETIME_FOR_ORPHAN) < Utc::now().timestamp() {
                        self.orphans.remove(orphan);
                    }
                }
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

        pms.update();

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

        pms.update();

        assert_eq!(pms.buffer.len(), 4000);
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

        pms.update();

        assert_eq!(pms.buffer.len(), SIZE_OF_MSGS_BUFFER);
        assert_eq!(pms.last_term(), 7000);
        assert_eq!(pms.orphans.len(), 0);

        //
        // Fill the buffer with random generated terms in range 7001..10001
        // This will occasionally update the buffer
        // At the end, the messages in the buffer have to be in correct order
        //
        let mut terms: Vec<u64> = (7001..10001).collect();
        terms.shuffle(&mut thread_rng());

        for term in terms {
            let privmsg = Privmsg::new("nick", "#dev", &format!("message_{}", term), term);
            pms.push(&privmsg);
            if rand::random() {
                pms.update();
            }
        }

        pms.update();

        assert_eq!(pms.buffer.len(), SIZE_OF_MSGS_BUFFER);
        assert_eq!(pms.last_term(), 10000);
        assert_eq!(pms.orphans.len(), 0);
    }
}
