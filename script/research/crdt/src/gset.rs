use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
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
