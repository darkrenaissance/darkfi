use async_std::sync::{Arc, Mutex};
use std::collections::VecDeque;

use crate::{Privmsg, SIZE_OF_MSGS_BUFFER};

pub struct RingBuffer<T> {
    pub items: VecDeque<T>,
    pub size: usize,
}

impl<T: Eq + PartialEq> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        let items = VecDeque::with_capacity(capacity);
        let size = items.capacity();
        Self { items, size }
    }

    pub fn push(&mut self, val: T) {
        if self.items.len() == self.size {
            self.items.pop_front();
        }
        self.items.push_back(val);
    }

    pub fn contains(&self, val: &T) -> bool {
        self.items.contains(val)
    }
}

pub type SeenMsgIds = Arc<Mutex<RingBuffer<u64>>>;

pub type ArcPrivmsgsBuffer = Arc<Mutex<PrivmsgsBuffer>>;

pub struct PrivmsgsBuffer(RingBuffer<Privmsg>);

impl PrivmsgsBuffer {
    pub fn new() -> ArcPrivmsgsBuffer {
        Arc::new(Mutex::new(Self(RingBuffer::new(SIZE_OF_MSGS_BUFFER))))
    }

    pub fn push(&mut self, _privmsg: &Privmsg) {
        // TODO
    }

    pub fn last_term(&self) -> u64 {
        match self.0.items.len() {
            0 => 0,
            n => self.0.items[n - 1].term,
        }
    }

    pub fn to_vec(&self) -> Vec<Privmsg> {
        self.0.items.clone().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_ring_buffer() {
        let mut b = RingBuffer::<&str>::new(3);
        b.push("h1");
        b.push("h2");
        b.push("h3");
        assert_eq!(b.items, vec!["h1", "h2", "h3"]);
        assert_eq!(b.items.capacity(), b.size);
        b.push("h4");
        assert_eq!(b.items, vec!["h2", "h3", "h4"]);
        assert_eq!(b.items.capacity(), b.size);
    }
}
