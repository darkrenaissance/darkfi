use std::cmp::max;

use serde::{Deserialize, Serialize};

use crate::{Event, GSet};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Node<T: Ord + Clone> {
    // name to idnetifie the node
    name: String,
    // a grow-only set
    pub(crate) gset: GSet<Event<T>>,
    // a counter for the node
    time: u64,
}

impl<T: Ord + Clone> Node<T> {
    pub fn new(name: &str) -> Self {
        Self { name: name.into(), gset: GSet::new(), time: 0 }
    }

    pub fn receive_event(&mut self, event: &Event<T>) {
        self.time = max(self.time, event.counter) + 1;
        self.gset.insert(event);
    }

    pub fn send_event(&mut self, value: &T) -> Event<T> {
        self.time += 1;
        let event = Event::new(value, self.time, self.name.clone());
        self.gset.insert(&event);
        event
    }
}
