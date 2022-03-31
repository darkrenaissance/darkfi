use std::{collections::BTreeSet, fmt::Debug};

use log::debug;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GSet<T: Ord> {
    set: BTreeSet<T>,
}

impl<T: Ord + Clone + Debug> GSet<T> {
    pub fn new() -> Self {
        Self { set: BTreeSet::new() }
    }

    pub fn insert(&mut self, element: &T) {
        debug!(target: "crdt", "GSet insert an element: {:?}", element);
        self.set.insert(element.clone());
    }

    pub fn contains(&self, element: &T) -> bool {
        self.set.contains(element)
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn merge(&mut self, other: &Self) {
        debug!(target: "crdt", "GSet merge a set of len: {:?}", other.len());
        other.set.iter().for_each(|e| self.insert(e))
    }

    pub fn get_set(&self) -> BTreeSet<T> {
        self.set.clone()
    }
}

impl<T: Ord + Clone + Debug> Default for GSet<T> {
    fn default() -> Self {
        Self::new()
    }
}
