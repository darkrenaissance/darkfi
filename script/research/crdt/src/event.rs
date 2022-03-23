use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd)]
pub struct Event<T: Clone> {
    // the msg in the event
    pub value: T,
    // the counter for lamport clock
    pub counter: u64,
    // It might be necessary to attach the node's name to the timestamp
    // so that it is possible to differentiate between events
    pub name: String,
}

impl<T: Clone> Event<T> {
    pub fn new(value: &T, counter: u64, name: String) -> Self {
        Self { value: value.clone(), counter, name }
    }
}

impl<T: Eq + PartialOrd + Clone> Ord for Event<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        let ord = self.counter.cmp(&other.counter);
        if ord == Ordering::Equal {
            return self.name.cmp(&other.name)
        }
        ord
    }
}
