use std::{
    cmp::{max, Ordering},
    collections::BTreeSet,
};

use serde::{Deserialize, Serialize};

// CRDT using gset and lamport clock

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
struct Node<T: Ord + Clone> {
    // name to idnetifie the node
    name: String,
    // a grow-only set
    gset: GSet<Event<T>>,
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd)]
struct Event<T: Clone> {
    // the msg in the event
    value: T,
    // the counter for lamport clock
    counter: u64,
    // It might be necessary to attach the node's name to the timestamp
    // so that it is possible to differentiate between events
    name: String,
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct GSet<T: Ord> {
    set: BTreeSet<T>,
}

impl<T: Ord + Clone> GSet<T> {
    pub fn new() -> Self {
        Self { set: BTreeSet::new() }
    }

    pub fn insert(&mut self, element: &T) {
        self.set.insert(element.clone());
    }

    pub fn contains(&self, element: &T) -> bool {
        self.set.contains(element)
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn merge(&mut self, other: &Self) {
        other.set.iter().for_each(|e| self.insert(e))
    }
}

impl<T: Ord + Clone> Default for GSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn sync_simulation(
        mut a: Node<String>,
        mut b: Node<String>,
        mut c: Node<String>,
    ) -> (Node<String>, Node<String>, Node<String>) {
        a.gset.merge(&b.gset);
        a.gset.merge(&c.gset);

        b.gset.merge(&a.gset);
        b.gset.merge(&c.gset);

        c.gset.merge(&a.gset);
        c.gset.merge(&b.gset);

        (a, b, c)
    }

    #[test]
    fn test_crdt_gset() {
        let mut a: Node<String> = Node::new("Node A");
        let mut b: Node<String> = Node::new("Node B");
        let mut c: Node<String> = Node::new("Node C");

        // node a
        a.send_event(&"a_msg1".to_string());
        a.send_event(&"a_msg2".to_string());

        // node b
        b.send_event(&"b_msg1".to_string());

        // node c
        c.send_event(&"c_msg1".to_string());

        // node b
        b.send_event(&"b_msg2".to_string());

        let (a, mut b, mut c) = sync_simulation(a, b, c);

        assert_eq!(a.gset.len(), 5);
        assert_eq!(b.gset.len(), 5);
        assert_eq!(c.gset.len(), 5);

        // node c
        c.send_event(&"c_msg2".to_string());
        c.send_event(&"c_msg3".to_string());
        c.send_event(&"c_msg4".to_string());
        c.send_event(&"c_msg5".to_string());

        // node b
        b.send_event(&"b_msg3".to_string());

        let (a, b, c) = sync_simulation(a, b, c);

        assert_eq!(a.gset.len(), 10);
        assert_eq!(b.gset.len(), 10);
        assert_eq!(c.gset.len(), 10);
    }
}
