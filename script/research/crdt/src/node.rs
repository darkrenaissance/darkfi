use std::cmp::max;

use darkfi::util::serial::{Decodable, Encodable};

use crate::{Event, GSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    // name to idnetifie the node
    name: String,
    // a grow-only set
    pub(crate) gset: GSet<Event>,
    // a counter for the node
    time: u64,
}

impl Node {
    pub fn new(name: &str) -> Self {
        Self { name: name.into(), gset: GSet::new(), time: 0 }
    }

    pub fn receive_event(&mut self, event: &Event) {
        self.time = max(self.time, event.counter) + 1;
        self.gset.insert(event);
    }

    pub fn send_event<T: Decodable + Encodable>(&mut self, value: T) -> Event {
        self.time += 1;
        let event = Event::new(&value, self.time, self.name.clone());
        self.gset.insert(&event);
        event
    }
}
