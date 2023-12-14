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

use std::{collections::VecDeque, iter::FusedIterator, mem};

/// Error handling
mod error;
use error::{DarkTreeError, DarkTreeResult};

#[cfg(test)]
mod tests;

/// This struct represents a Leaf of a [`DarkTree`],
/// holding this tree node data, along with positional
/// indexes information, based on tree's traversal order.
/// These indexes are only here to enable referencing
/// connected nodes, and are *not* used as pointers by the
/// tree. Creator must ensure they are properly setup.
#[derive(Debug, PartialEq)]
struct DarkLeaf<T> {
    /// Data holded by this leaf
    data: T,
    /// Index showcasing this leaf's position, when all
    /// leafs are in order.
    index: usize,
    /// Index showcasing this leaf's parent tree, when all
    /// leafs are in order. None indicates that this leaf
    /// has no parent.
    parent_index: Option<usize>,
    /// Vector of indexes showcasing this leaf's children
    /// positions, when all leafs are in order. If vector
    /// is empty, it indicates that this leaf has no children.
    children_indexes: Vec<usize>,
}

impl<T> DarkLeaf<T> {
    /// Every [`DarkLeaf`] is initiated using default indexes.
    fn new(data: T) -> DarkLeaf<T> {
        Self { data, index: 0, parent_index: None, children_indexes: vec![] }
    }

    /// Set [`DarkLeaf`]'s index
    fn set_index(&mut self, index: usize) {
        self.index = index;
    }

    /// Set [`DarkLeaf`]'s parent index
    fn set_parent_index(&mut self, parent_index: Option<usize>) {
        self.parent_index = parent_index;
    }

    /// Set [`DarkLeaf`]'s children index
    fn set_children_indexes(&mut self, children_indexes: Vec<usize>) {
        self.children_indexes = children_indexes;
    }
}

/// This struct represents a Tree using DFS post-order traversal,
/// where when we iterate through the tree, we first process tree
/// node's children, and then the node itself, recursively.
/// Based on this, initial tree node (leaf), known as the root,
/// will always show up at the end of iteration. It is advised
/// to always execute .build() after finishing setting up the
/// Tree, to properly index it and check its integrity.
#[derive(Debug, PartialEq)]
struct DarkTree<T> {
    /// This tree's leaf information, along with its data
    leaf: DarkLeaf<T>,
    /// Vector containing all tree's branches(children tree)
    children: Vec<DarkTree<T>>,
    /// Min capacity of the tree, including all children nodes
    /// recursively from the root. Since root is always present,
    /// min capacity must always be >= 1. This is enforced by
    /// the root, so children nodes don't have to set it up.
    /// If children nodes children(recursively) make us not exceed
    /// that min capacity, we will be able to catch it using
    /// .check_min_capacity() or .integrity_check().
    min_capacity: usize,
    /// Optional max capacity of the tree, including all children
    /// nodes recursively from the root. None indicates no
    /// capacity restrictions. This is enforced by the root,
    /// so children nodes don't have to set it up. If children
    /// nodes children(recursively) make us exceed that capacity,
    /// we will be able to catch it using .check_capacity() or
    /// .integrity_check().
    max_capacity: Option<usize>,
}

impl<T> DarkTree<T> {
    /// Initialize a [`DarkTree`], using provided data to
    /// generate its root.
    fn new(
        data: T,
        children: Vec<DarkTree<T>>,
        min_capacity: Option<usize>,
        max_capacity: Option<usize>,
    ) -> DarkTree<T> {
        // Setup min capacity
        let min_capacity = if let Some(min_capacity) = min_capacity {
            if min_capacity == 0 {
                1
            } else {
                min_capacity
            }
        } else {
            1
        };
        let leaf = DarkLeaf::new(data);
        Self { leaf, children, min_capacity, max_capacity }
    }

    /// Build the [`DarkTree`] indexes and perform an
    /// integrity check on them. This should be used
    /// after we have appended all child nodes, so we
    /// don't have to call .index() and .integrity_check()
    /// manually.
    fn build(&mut self) -> DarkTreeResult<()> {
        self.index();
        self.integrity_check()
    }

    /// Return the count of all [`DarkTree`] leafs.
    fn len(&self) -> usize {
        self.iter().count()
    }

    /// Check if configured min capacity have been exceeded.
    fn check_min_capacity(&self) -> DarkTreeResult<()> {
        if self.len() < self.min_capacity {
            return Err(DarkTreeError::MinCapacityNotExceeded)
        }

        Ok(())
    }

    /// Check if configured max capacity have been exceeded.
    fn check_max_capacity(&self) -> DarkTreeResult<()> {
        if let Some(max_capacity) = self.max_capacity {
            if self.len() >= max_capacity {
                return Err(DarkTreeError::MaxCapacityExceeded)
            }
        }

        Ok(())
    }

    /// Append a new child node to the [`DarkTree`],
    /// if max capacity has not been exceeded. This call
    /// doesn't update the indexes, so either .index()
    /// or .build() must be called after it.
    fn append(&mut self, child: DarkTree<T>) -> DarkTreeResult<()> {
        // Check current max capacity
        self.check_max_capacity()?;

        // Append the new child
        self.children.push(child);

        Ok(())
    }

    /// Set [`DarkTree`]'s leaf parent and children indexes,
    /// and trigger the setup of its children indexes.
    fn set_parent_children_indexes(&mut self, parent_index: Option<usize>) {
        // Set our leafs parent index
        self.leaf.set_parent_index(parent_index);

        // Now recursively, we setup nodes children indexes and keep
        // their index in our own children index list
        let mut children_indexes = vec![];
        for child in &mut self.children {
            child.set_parent_children_indexes(Some(self.leaf.index));
            children_indexes.push(child.leaf.index);
        }

        // Set our leafs children indexes
        self.leaf.set_children_indexes(children_indexes);
    }

    /// Setup [`DarkTree`]'s leafs indexes, based on DFS post-order
    /// traversal order. This call assumes it was triggered for the
    /// root of the tree, which has no parent index.
    fn index(&mut self) {
        // First we setup each leafs index
        for (index, leaf) in self.iter_mut().enumerate() {
            leaf.set_index(index);
        }

        // Now we trigger recursion to setup each nodes rest indexes
        self.set_parent_children_indexes(None);
    }

    /// Verify [`DarkTree`]'s leaf parent and children indexes validity,
    /// and trigger the check of its children indexes.
    fn check_parent_children_indexes(&self, parent_index: Option<usize>) -> DarkTreeResult<()> {
        // Check our leafs parent index
        if self.leaf.parent_index != parent_index {
            return Err(DarkTreeError::InvalidLeafParentIndex(self.leaf.index))
        }

        // Now recursively, we check nodes children indexes and keep
        // their index in our own children index list
        let mut children_indexes = vec![];
        for child in &self.children {
            child.check_parent_children_indexes(Some(self.leaf.index))?;
            children_indexes.push(child.leaf.index);
        }

        // Check our leafs children indexes
        if self.leaf.children_indexes != children_indexes {
            return Err(DarkTreeError::InvalidLeafChildrenIndexes(self.leaf.index))
        }

        Ok(())
    }

    /// Verify current [`DarkTree`]'s leafs indexes validity,
    /// based on DFS post-order traversal order. Additionally,
    /// check that min and max capacities have been properly
    /// configured, min capacity has been exceeded and max
    /// capacity has not. This call assumes it was triggered
    /// for the root of the tree, which has no parent index.
    fn integrity_check(&self) -> DarkTreeResult<()> {
        // Check current min capacity is valid
        if self.min_capacity < 1 {
            return Err(DarkTreeError::InvalidMinCapacity(self.min_capacity))
        }

        // Check currect max capacity is not less than
        // current min capacity
        if let Some(max_capacity) = self.max_capacity {
            if self.min_capacity > max_capacity {
                return Err(DarkTreeError::InvalidMaxCapacity(max_capacity, self.min_capacity))
            }
        }

        // Check current min capacity
        self.check_min_capacity()?;

        // Check current max capacity
        self.check_max_capacity()?;

        // Check each leaf index
        for (index, leaf) in self.iter().enumerate() {
            if index != leaf.index {
                return Err(DarkTreeError::InvalidLeafIndex(leaf.index, index))
            }
        }

        // Trigger recursion to check each nodes rest indexes
        self.check_parent_children_indexes(None)
    }

    /// Immutably iterate through the tree, using DFS post-order
    /// traversal.
    fn iter(&self) -> DarkTreeIter<'_, T> {
        DarkTreeIter { children: std::slice::from_ref(self), parent: None }
    }

    /// Mutably iterate through the tree, using DFS post-order
    /// traversal.
    fn iter_mut(&mut self) -> DarkTreeIterMut<'_, T> {
        DarkTreeIterMut { children: std::slice::from_mut(self), parent: None, parent_leaf: None }
    }
}

/// Immutable iterator of a [`DarkTree`], performing DFS post-order
/// traversal on the Tree leafs.
struct DarkTreeIter<'a, T> {
    children: &'a [DarkTree<T>],
    parent: Option<Box<DarkTreeIter<'a, T>>>,
}

impl<T> Default for DarkTreeIter<'_, T> {
    fn default() -> Self {
        DarkTreeIter { children: &[], parent: None }
    }
}

impl<'a, T> Iterator for DarkTreeIter<'a, T> {
    type Item = &'a DarkLeaf<T>;

    /// Grab next item iterator visits and return
    /// its immutable reference, or recursively
    /// create and continue iteration on current
    /// leaf's children.
    fn next(&mut self) -> Option<Self::Item> {
        match self.children.first() {
            None => match self.parent.take() {
                Some(parent) => {
                    // Grab parent's leaf
                    *self = *parent;
                    // Its safe to unwrap here as we effectively returned
                    // to this tree after "pushing" it after its children
                    let leaf = &self.children.first().unwrap().leaf;
                    self.children = &self.children[1..];
                    Some(leaf)
                }
                None => None,
            },
            Some(leaf) => {
                // Iterate over tree's children/sub-trees
                *self = DarkTreeIter {
                    children: leaf.children.as_slice(),
                    parent: Some(Box::new(mem::take(self))),
                };
                self.next()
            }
        }
    }
}

impl<T> FusedIterator for DarkTreeIter<'_, T> {}

/// Define fusion iteration behavior, allowing
/// us to use the [`DarkTreeIter`] iterator in
/// loops directly, without using .iter() method
/// of [`DarkTree`].
impl<'a, T> IntoIterator for &'a DarkTree<T> {
    type Item = &'a DarkLeaf<T>;

    type IntoIter = DarkTreeIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Mutable iterator of a [`DarkTree`], performing DFS post-order
/// traversal on the Tree leafs.
struct DarkTreeIterMut<'a, T> {
    children: &'a mut [DarkTree<T>],
    parent: Option<Box<DarkTreeIterMut<'a, T>>>,
    parent_leaf: Option<&'a mut DarkLeaf<T>>,
}

impl<T> Default for DarkTreeIterMut<'_, T> {
    fn default() -> Self {
        DarkTreeIterMut { children: &mut [], parent: None, parent_leaf: None }
    }
}

impl<'a, T> Iterator for DarkTreeIterMut<'a, T> {
    type Item = &'a mut DarkLeaf<T>;

    /// Grab next item iterator visits and return
    /// its mutable reference, or recursively
    /// create and continue iteration on current
    /// leaf's children.
    fn next(&mut self) -> Option<Self::Item> {
        let children = mem::take(&mut self.children);
        match children.split_first_mut() {
            None => match self.parent.take() {
                Some(parent) => {
                    // Grab parent's leaf
                    let parent_leaf = mem::take(&mut self.parent_leaf);
                    *self = *parent;
                    parent_leaf
                }
                None => None,
            },
            Some((first, rest)) => {
                // Setup simplings iteration
                self.children = rest;

                // Iterate over tree's children/sub-trees
                *self = DarkTreeIterMut {
                    children: first.children.as_mut_slice(),
                    parent: Some(Box::new(mem::take(self))),
                    parent_leaf: Some(&mut first.leaf),
                };
                self.next()
            }
        }
    }
}

/// Define fusion iteration behavior, allowing
/// us to use the [`DarkTreeIterMut`] iterator
/// in loops directly, without using .iter_mut()
/// method of [`DarkTree`].
impl<'a, T> IntoIterator for &'a mut DarkTree<T> {
    type Item = &'a mut DarkLeaf<T>;

    type IntoIter = DarkTreeIterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// Special iterator of a [`DarkTree`], performing DFS post-order
/// traversal on the Tree leafs, consuming each leaf. Since this
/// iterator consumes the tree, it becomes unusable after it's moved.
struct DarkTreeIntoIter<T> {
    children: VecDeque<DarkTree<T>>,
    parent: Option<Box<DarkTreeIntoIter<T>>>,
}

impl<T> Default for DarkTreeIntoIter<T> {
    fn default() -> Self {
        DarkTreeIntoIter { children: Default::default(), parent: None }
    }
}

impl<T> Iterator for DarkTreeIntoIter<T> {
    type Item = DarkLeaf<T>;

    /// Move next item iterator visits from the tree
    /// to the iterator consumer, if it has no children.
    /// Otherwise recursively create and continue iteration
    /// on current leaf's children, and moving it after them.
    fn next(&mut self) -> Option<Self::Item> {
        match self.children.pop_front() {
            None => match self.parent.take() {
                Some(parent) => {
                    // Continue iteration on parent's simplings
                    *self = *parent;
                    self.next()
                }
                None => None,
            },
            Some(mut leaf) => {
                // If leaf has no children, return it
                if leaf.children.is_empty() {
                    return Some(leaf.leaf)
                }

                // Push leaf after its children
                let mut children: VecDeque<DarkTree<T>> = leaf.children.into();
                leaf.children = Default::default();
                children.push_back(leaf);

                // Iterate over tree's children/sub-trees
                *self = DarkTreeIntoIter { children, parent: Some(Box::new(mem::take(self))) };
                self.next()
            }
        }
    }
}

impl<T> FusedIterator for DarkTreeIntoIter<T> {}

/// Define fusion iteration behavior, allowing
/// us to use the [`DarkTreeIntoIter`] .into_iter()
/// method, to consume the [`DarkTree`] and iterate
/// over it.
impl<T> IntoIterator for DarkTree<T> {
    type Item = DarkLeaf<T>;

    type IntoIter = DarkTreeIntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        let mut children = VecDeque::with_capacity(1);
        children.push_back(self);

        DarkTreeIntoIter { children, parent: None }
    }
}
