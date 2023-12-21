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

use std::{
    clone::Clone,
    collections::VecDeque,
    iter::FusedIterator,
    marker::{Send, Sync},
    mem,
};

#[cfg(feature = "async")]
use darkfi_serial::async_trait;
use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::error::{DarkTreeError, DarkTreeResult};

/// This struct represents the information hold by a
/// [`DarkTreeLeaf`], namely its data, along with positional
/// indexes information, based on tree's traversal order.
/// These indexes are only here to enable referencing
/// connected nodes, and are *not* used as pointers by the
/// tree. Creator must ensure they are properly setup.
#[derive(Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct DarkLeaf<T>
where
    T: Clone + Send + Sync,
{
    /// Data holded by this leaf
    pub data: T,
    /// Index showcasing this leaf's parent tree, when all
    /// leafs are in order. None indicates that this leaf
    /// has no parent.
    pub parent_index: Option<usize>,
    /// Vector of indexes showcasing this leaf's children
    /// positions, when all leafs are in order. If vector
    /// is empty, it indicates that this leaf has no children.
    pub children_indexes: Vec<usize>,
}

/// This struct represents a Leaf of a [`DarkTree`],
/// holding this tree node data, along with its positional
/// index, based on tree's traversal order.
#[derive(Clone, Debug, PartialEq)]
pub struct DarkTreeLeaf<T>
where
    T: Clone + Send + Sync,
{
    /// Index showcasing this leaf's position, when all
    /// leafs are in order.
    index: usize,
    /// Leaf's data, along with its parent and children
    /// indexes information.
    info: DarkLeaf<T>,
}

impl<T: Clone + Send + Sync> DarkTreeLeaf<T> {
    /// Every [`DarkTreeLeaf`] is initiated using default indexes.
    fn new(data: T) -> DarkTreeLeaf<T> {
        Self { index: 0, info: DarkLeaf { data, parent_index: None, children_indexes: vec![] } }
    }

    /// Set [`DarkTreeLeaf`]'s index
    fn set_index(&mut self, index: usize) {
        self.index = index;
    }

    /// Set [`DarkTreeLeaf`]'s parent index
    fn set_parent_index(&mut self, parent_index: Option<usize>) {
        self.info.parent_index = parent_index;
    }

    /// Set [`DarkTreeLeaf`]'s children index
    fn set_children_indexes(&mut self, children_indexes: Vec<usize>) {
        self.info.children_indexes = children_indexes;
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
pub struct DarkTree<T: Clone + Send + Sync> {
    /// This tree's leaf information, along with its data
    leaf: DarkTreeLeaf<T>,
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
    /// we will be able to catch it using .check_max_capacity() or
    /// .integrity_check().
    max_capacity: Option<usize>,
}

impl<T: Clone + Send + Sync> DarkTree<T> {
    /// Initialize a [`DarkTree`], using provided data to
    /// generate its root.
    pub fn new(
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
        let leaf = DarkTreeLeaf::new(data);
        Self { leaf, children, min_capacity, max_capacity }
    }

    /// Build the [`DarkTree`] indexes and perform an
    /// integrity check on them. This should be used
    /// after we have appended all child nodes, so we
    /// don't have to call .index() and .integrity_check()
    /// manually.
    pub fn build(&mut self) -> DarkTreeResult<()> {
        self.index();
        self.integrity_check()
    }

    /// Build the [`DarkTree`] using .build() and
    /// then produce a flattened vector containing
    /// all the leafs in DFS post-order traversal order.
    pub fn build_vec(&mut self) -> DarkTreeResult<Vec<DarkLeaf<T>>> {
        self.build()?;
        Ok(self.iter().cloned().map(|x| x.info).collect())
    }

    /// Build the [`DarkTree`] flattened vector using
    /// .build_vec() and then move the root from last
    /// position to first, updating all indexes
    pub fn build_shifted_root_vec(&mut self) -> DarkTreeResult<Vec<DarkLeaf<T>>> {
        let mut vec = self.build_vec()?;

        // Keep initial root index
        let root_index = vec.len() - 1;

        // Grab root and update its children indexes
        let mut root = vec.pop().unwrap();
        for child in &mut root.children_indexes {
            *child += 1;
        }

        // Update rest indexes by shifting 1 position
        for leaf in &mut vec {
            if let Some(parent) = &mut leaf.parent_index {
                if *parent == root_index {
                    *parent = 0;
                } else {
                    *parent += 1;
                }
            }
            for child in &mut leaf.children_indexes {
                *child += 1;
            }
        }

        // Push root in the front of the vec
        vec.insert(0, root);

        Ok(vec)
    }

    /// Return the count of all [`DarkTree`] leafs.
    fn len(&self) -> usize {
        self.iter().count()
    }

    /// Check if configured min capacity have not been exceeded.
    fn check_min_capacity(&self) -> DarkTreeResult<()> {
        if self.len() < self.min_capacity {
            return Err(DarkTreeError::MinCapacityNotExceeded)
        }

        Ok(())
    }

    /// Check if configured max capacity have been exceeded.
    fn check_max_capacity(&self) -> DarkTreeResult<()> {
        if let Some(max_capacity) = self.max_capacity {
            if self.len() > max_capacity {
                return Err(DarkTreeError::MaxCapacityExceeded)
            }
        }

        Ok(())
    }

    /// Append a new child node to the [`DarkTree`],
    /// if max capacity has not been exceeded. This call
    /// doesn't update the indexes, so either .index()
    /// or .build() must be called after it.
    pub fn append(&mut self, child: DarkTree<T>) -> DarkTreeResult<()> {
        // Check current max capacity
        if let Some(max_capacity) = self.max_capacity {
            if self.len() + 1 > max_capacity {
                return Err(DarkTreeError::MaxCapacityExceeded)
            }
        }

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
        if self.leaf.info.parent_index != parent_index {
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
        if self.leaf.info.children_indexes != children_indexes {
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
pub struct DarkTreeIter<'a, T: Clone + Send + Sync> {
    children: &'a [DarkTree<T>],
    parent: Option<Box<DarkTreeIter<'a, T>>>,
}

impl<T: Clone + Send + Sync> Default for DarkTreeIter<'_, T> {
    fn default() -> Self {
        DarkTreeIter { children: &[], parent: None }
    }
}

impl<'a, T: Clone + Send + Sync> Iterator for DarkTreeIter<'a, T> {
    type Item = &'a DarkTreeLeaf<T>;

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

impl<T: Clone + Send + Sync> FusedIterator for DarkTreeIter<'_, T> {}

/// Define fusion iteration behavior, allowing
/// us to use the [`DarkTreeIter`] iterator in
/// loops directly, without using .iter() method
/// of [`DarkTree`].
impl<'a, T: Clone + Send + Sync> IntoIterator for &'a DarkTree<T> {
    type Item = &'a DarkTreeLeaf<T>;

    type IntoIter = DarkTreeIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Mutable iterator of a [`DarkTree`], performing DFS post-order
/// traversal on the Tree leafs.
pub struct DarkTreeIterMut<'a, T: Clone + Send + Sync> {
    children: &'a mut [DarkTree<T>],
    parent: Option<Box<DarkTreeIterMut<'a, T>>>,
    parent_leaf: Option<&'a mut DarkTreeLeaf<T>>,
}

impl<T: Clone + Send + Sync> Default for DarkTreeIterMut<'_, T> {
    fn default() -> Self {
        DarkTreeIterMut { children: &mut [], parent: None, parent_leaf: None }
    }
}

impl<'a, T: Clone + Send + Sync> Iterator for DarkTreeIterMut<'a, T> {
    type Item = &'a mut DarkTreeLeaf<T>;

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
impl<'a, T: Clone + Send + Sync> IntoIterator for &'a mut DarkTree<T> {
    type Item = &'a mut DarkTreeLeaf<T>;

    type IntoIter = DarkTreeIterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// Special iterator of a [`DarkTree`], performing DFS post-order
/// traversal on the Tree leafs, consuming each leaf. Since this
/// iterator consumes the tree, it becomes unusable after it's moved.
pub struct DarkTreeIntoIter<T: Clone + Send + Sync> {
    children: VecDeque<DarkTree<T>>,
    parent: Option<Box<DarkTreeIntoIter<T>>>,
}

impl<T: Clone + Send + Sync> Default for DarkTreeIntoIter<T> {
    fn default() -> Self {
        DarkTreeIntoIter { children: Default::default(), parent: None }
    }
}

impl<T: Clone + Send + Sync> Iterator for DarkTreeIntoIter<T> {
    type Item = DarkTreeLeaf<T>;

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

impl<T: Clone + Send + Sync> FusedIterator for DarkTreeIntoIter<T> {}

/// Define fusion iteration behavior, allowing
/// us to use the [`DarkTreeIntoIter`] .into_iter()
/// method, to consume the [`DarkTree`] and iterate
/// over it.
impl<T: Clone + Send + Sync> IntoIterator for DarkTree<T> {
    type Item = DarkTreeLeaf<T>;

    type IntoIter = DarkTreeIntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        let mut children = VecDeque::with_capacity(1);
        children.push_back(self);

        DarkTreeIntoIter { children, parent: None }
    }
}

/// Auxiliary function to verify provided [`DarkLeaf`] slice is
/// properly bounded and its members indexes are valid.
pub fn dark_leaf_vec_integrity_check<T: Clone + Send + Sync>(
    leafs: &[DarkLeaf<T>],
    min_capacity: Option<usize>,
    max_capacity: Option<usize>,
    shifted_root: bool,
) -> DarkTreeResult<()> {
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

    // Check currect max capacity is not less than
    // current min capacity
    if let Some(max_capacity) = max_capacity {
        if min_capacity > max_capacity {
            return Err(DarkTreeError::InvalidMaxCapacity(max_capacity, min_capacity))
        }
    }

    // Check if min capacity have been not exceeded
    if leafs.len() < min_capacity {
        return Err(DarkTreeError::MinCapacityNotExceeded)
    }

    // Check if max capacity have been exceeded
    if let Some(max_capacity) = max_capacity {
        if leafs.len() > max_capacity {
            return Err(DarkTreeError::MaxCapacityExceeded)
        }
    }

    // Grab correct range iterator to exclude root
    let iterator = if shifted_root { leafs[1..].iter() } else { leafs[..leafs.len() - 1].iter() };

    // Check each leaf indexes exluding root
    let mut checked_indexes = Vec::with_capacity(leafs.len());
    for (mut index, leaf) in iterator.enumerate() {
        // Shift index
        if shifted_root {
            index += 1;
        }

        // Check parent index exists
        let Some(parent_index) = leaf.parent_index else {
            return Err(DarkTreeError::InvalidLeafParentIndex(index))
        };

        // Parent index is not out of bounds
        if parent_index > leafs.len() - 1 {
            return Err(DarkTreeError::InvalidLeafParentIndex(index))
        }

        // Our index must be less than our parent index, excluding root
        // when shifted
        if (parent_index != 0 || !shifted_root) && index >= parent_index {
            return Err(DarkTreeError::InvalidLeafParentIndex(index))
        }

        // Parent must have our index in their children
        if !leafs[parent_index].children_indexes.contains(&index) {
            return Err(DarkTreeError::InvalidLeafChildrenIndexes(parent_index))
        }

        // Check children indexes validity
        check_children(leafs, &index, leaf, &checked_indexes, shifted_root)?;

        checked_indexes.push(index);
    }

    // It's safe to unwrap here since we enforced min capacity of 1
    let (root, root_index) =
        if shifted_root { (&leafs[0], 0) } else { (leafs.last().unwrap(), leafs.len() - 1) };

    // Root must not contain a parent
    if root.parent_index.is_some() {
        return Err(DarkTreeError::InvalidLeafParentIndex(leafs.len() - 1))
    }

    // Check its children
    check_children(leafs, &root_index, root, &checked_indexes, shifted_root)
}

/// Check `DarkLeaf` children indexes validity
fn check_children<T: Clone + Send + Sync>(
    leafs: &[DarkLeaf<T>],
    index: &usize,
    leaf: &DarkLeaf<T>,
    checked_indexes: &[usize],
    shifted_root: bool,
) -> DarkTreeResult<()> {
    let mut children_vec = Vec::with_capacity(leaf.children_indexes.len());
    for child_index in &leaf.children_indexes {
        // Children vector must be sorted and don't contain duplicates
        if let Some(last) = children_vec.last() {
            if child_index <= last {
                return Err(DarkTreeError::InvalidLeafChildrenIndexes(*index))
            }
        }

        // We must have already checked that child
        if !checked_indexes.contains(child_index) {
            return Err(DarkTreeError::InvalidLeafChildrenIndexes(*index))
        }

        // Our index must be greater than our child index, excluding root
        // when shifted
        if (*index != 0 || !shifted_root) && index <= child_index {
            return Err(DarkTreeError::InvalidLeafChildrenIndexes(*index))
        }

        // Children must have its parent set to us
        match leafs[*child_index].parent_index {
            Some(parent_index) => {
                if parent_index != *index {
                    return Err(DarkTreeError::InvalidLeafParentIndex(*child_index))
                }
            }
            None => return Err(DarkTreeError::InvalidLeafParentIndex(*child_index)),
        }

        children_vec.push(*child_index);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gereate a predefined [`DarkTree`] along with its
    /// expected traversal order.
    ///
    /// Tree structure:
    ///                        22
    ///           /             |            \
    ///          10            14            21
    ///      /  /  \   \      /  \      /     |   \
    ///     2   4   6   9    12  13    17    18   20
    ///    / \  |   |  / \   |        /  \         |
    ///   0  1  3   5 7   8  11      15  16       19
    ///
    /// Expected traversal order is indicated by each leaf's number
    fn generate_tree() -> DarkTreeResult<(DarkTree<i32>, Vec<i32>)> {
        let mut tree = DarkTree::new(
            22,
            vec![
                DarkTree::new(
                    10,
                    vec![
                        DarkTree::new(
                            2,
                            vec![
                                DarkTree::new(0, vec![], None, None),
                                DarkTree::new(1, vec![], None, None),
                            ],
                            None,
                            None,
                        ),
                        DarkTree::new(4, vec![DarkTree::new(3, vec![], None, None)], None, None),
                        DarkTree::new(6, vec![DarkTree::new(5, vec![], None, None)], None, None),
                        DarkTree::new(
                            9,
                            vec![
                                DarkTree::new(7, vec![], None, None),
                                DarkTree::new(8, vec![], None, None),
                            ],
                            None,
                            None,
                        ),
                    ],
                    None,
                    None,
                ),
                DarkTree::new(
                    14,
                    vec![
                        DarkTree::new(12, vec![DarkTree::new(11, vec![], None, None)], None, None),
                        DarkTree::new(13, vec![], None, None),
                    ],
                    None,
                    None,
                ),
                DarkTree::new(
                    21,
                    vec![
                        DarkTree::new(
                            17,
                            vec![
                                DarkTree::new(15, vec![], None, None),
                                DarkTree::new(16, vec![], None, None),
                            ],
                            None,
                            None,
                        ),
                        DarkTree::new(18, vec![], None, None),
                        DarkTree::new(20, vec![DarkTree::new(19, vec![], None, None)], None, None),
                    ],
                    None,
                    None,
                ),
            ],
            None,
            None,
        );

        tree.build()?;

        let traversal_order = (0..23).collect();

        Ok((tree, traversal_order))
    }

    #[test]
    fn test_darktree_iterator() -> DarkTreeResult<()> {
        let (tree, traversal_order) = generate_tree()?;

        // Use [`DarkTree`] iterator to collect current
        // data, in order
        let nums: Vec<i32> = tree.iter().map(|x| x.info.data).collect();

        // Verify iterator collected the data in the expected
        // traversal order.
        assert_eq!(nums, traversal_order);

        // Verify using iterator indexing methods to retrieve
        // data from it, returns the expected one, as per
        // expected traversal order.
        assert_eq!(tree.iter().nth(1).unwrap().info.data, traversal_order[1]);

        // Thanks for reading
        Ok(())
    }

    #[test]
    fn test_darktree_traversal_order() -> DarkTreeResult<()> {
        let (mut tree, traversal_order) = generate_tree()?;

        // Loop using the fusion immutable iterator,
        // verifying we grab the correct [`DarkTreeLeaf`]
        // immutable reference, as per expected
        // traversal order.
        let mut index = 0;
        for leaf in &tree {
            assert_eq!(leaf.info.data, traversal_order[index]);
            index += 1;
        }

        // Loop using the fusion mutable iterator,
        // verifying we grab the correct [`DarkTreeLeaf`]
        // mutable reference, as per expected traversal
        // order.
        index = 0;
        for leaf in &mut tree {
            assert_eq!(leaf.info.data, traversal_order[index]);
            index += 1;
        }

        // Loop using [`DarkTree`] .iter_mut() mutable
        // iterator, verifying we grab the correct [`DarkTreeLeaf`]
        // mutable reference, as per expected traversal
        // order.
        for (index, leaf) in tree.iter_mut().enumerate() {
            assert_eq!(leaf.info.data, traversal_order[index]);
        }

        // Loop using [`DarkTree`] .iter() immutable
        // iterator, verifying we grab the correct [`DarkTreeLeaf`]
        // immutable reference, as per expected traversal
        // order.
        for (index, leaf) in tree.iter().enumerate() {
            assert_eq!(leaf.info.data, traversal_order[index]);
        }

        // Loop using [`DarkTree`] .into_iter() iterator,
        // which consumes (moves) the tree, verifying we
        // collect the correct [`DarkTreeLeaf`], as per expected
        // traversal order.
        for (index, leaf) in tree.into_iter().enumerate() {
            assert_eq!(leaf.info.data, traversal_order[index]);
        }

        // Thanks for reading
        Ok(())
    }

    #[test]
    fn test_darktree_mut_iterator() -> DarkTreeResult<()> {
        let (mut tree, _) = generate_tree()?;

        // Loop using [`DarkTree`] .iter_mut() mutable
        // iterator, grabing a mutable reference over a
        // [`DarkTreeLeaf`], and mutating its inner data.
        for leaf in tree.iter_mut() {
            leaf.info.data += 1;
        }

        // Loop using the fusion mutable iterator,
        // grabing a mutable reference over a
        // [`DarkTreeLeaf`], and mutating its inner data.
        for leaf in &mut tree {
            leaf.info.data += 1;
        }

        // Verify performed mutation actually happened
        // on original tree. Additionally we verify all
        // indexes are the expected ones.
        assert_eq!(
            tree,
            DarkTree {
                leaf: DarkTreeLeaf {
                    index: 22,
                    info: DarkLeaf {
                        data: 24,
                        parent_index: None,
                        children_indexes: vec![10, 14, 21]
                    },
                },
                children: vec![
                    DarkTree {
                        leaf: DarkTreeLeaf {
                            index: 10,
                            info: DarkLeaf {
                                data: 12,
                                parent_index: Some(22),
                                children_indexes: vec![2, 4, 6, 9],
                            },
                        },
                        children: vec![
                            DarkTree {
                                leaf: DarkTreeLeaf {
                                    index: 2,
                                    info: DarkLeaf {
                                        data: 4,
                                        parent_index: Some(10),
                                        children_indexes: vec![0, 1],
                                    },
                                },
                                children: vec![
                                    DarkTree {
                                        leaf: DarkTreeLeaf {
                                            index: 0,
                                            info: DarkLeaf {
                                                data: 2,
                                                parent_index: Some(2),
                                                children_indexes: vec![],
                                            },
                                        },
                                        children: vec![],
                                        min_capacity: 1,
                                        max_capacity: None,
                                    },
                                    DarkTree {
                                        leaf: DarkTreeLeaf {
                                            index: 1,
                                            info: DarkLeaf {
                                                data: 3,
                                                parent_index: Some(2),
                                                children_indexes: vec![],
                                            },
                                        },
                                        children: vec![],
                                        min_capacity: 1,
                                        max_capacity: None,
                                    },
                                ],
                                min_capacity: 1,
                                max_capacity: None,
                            },
                            DarkTree {
                                leaf: DarkTreeLeaf {
                                    index: 4,
                                    info: DarkLeaf {
                                        data: 6,
                                        parent_index: Some(10),
                                        children_indexes: vec![3],
                                    },
                                },
                                children: vec![DarkTree {
                                    leaf: DarkTreeLeaf {
                                        index: 3,
                                        info: DarkLeaf {
                                            data: 5,
                                            parent_index: Some(4),
                                            children_indexes: vec![],
                                        },
                                    },
                                    children: vec![],
                                    min_capacity: 1,
                                    max_capacity: None,
                                },],
                                min_capacity: 1,
                                max_capacity: None,
                            },
                            DarkTree {
                                leaf: DarkTreeLeaf {
                                    index: 6,
                                    info: DarkLeaf {
                                        data: 8,
                                        parent_index: Some(10),
                                        children_indexes: vec![5],
                                    },
                                },
                                children: vec![DarkTree {
                                    leaf: DarkTreeLeaf {
                                        index: 5,
                                        info: DarkLeaf {
                                            data: 7,
                                            parent_index: Some(6),
                                            children_indexes: vec![],
                                        },
                                    },
                                    children: vec![],
                                    min_capacity: 1,
                                    max_capacity: None,
                                },],
                                min_capacity: 1,
                                max_capacity: None,
                            },
                            DarkTree {
                                leaf: DarkTreeLeaf {
                                    index: 9,
                                    info: DarkLeaf {
                                        data: 11,
                                        parent_index: Some(10),
                                        children_indexes: vec![7, 8],
                                    },
                                },
                                children: vec![
                                    DarkTree {
                                        leaf: DarkTreeLeaf {
                                            index: 7,
                                            info: DarkLeaf {
                                                data: 9,
                                                parent_index: Some(9),
                                                children_indexes: vec![],
                                            },
                                        },
                                        children: vec![],
                                        min_capacity: 1,
                                        max_capacity: None,
                                    },
                                    DarkTree {
                                        leaf: DarkTreeLeaf {
                                            index: 8,
                                            info: DarkLeaf {
                                                data: 10,
                                                parent_index: Some(9),
                                                children_indexes: vec![],
                                            },
                                        },
                                        children: vec![],
                                        min_capacity: 1,
                                        max_capacity: None,
                                    },
                                ],
                                min_capacity: 1,
                                max_capacity: None,
                            },
                        ],
                        min_capacity: 1,
                        max_capacity: None,
                    },
                    DarkTree {
                        leaf: DarkTreeLeaf {
                            index: 14,
                            info: DarkLeaf {
                                data: 16,
                                parent_index: Some(22),
                                children_indexes: vec![12, 13],
                            },
                        },
                        children: vec![
                            DarkTree {
                                leaf: DarkTreeLeaf {
                                    index: 12,
                                    info: DarkLeaf {
                                        data: 14,
                                        parent_index: Some(14),
                                        children_indexes: vec![11],
                                    },
                                },
                                children: vec![DarkTree {
                                    leaf: DarkTreeLeaf {
                                        index: 11,
                                        info: DarkLeaf {
                                            data: 13,
                                            parent_index: Some(12),
                                            children_indexes: vec![],
                                        },
                                    },
                                    children: vec![],
                                    min_capacity: 1,
                                    max_capacity: None,
                                },],
                                min_capacity: 1,
                                max_capacity: None,
                            },
                            DarkTree {
                                leaf: DarkTreeLeaf {
                                    index: 13,
                                    info: DarkLeaf {
                                        data: 15,
                                        parent_index: Some(14),
                                        children_indexes: vec![],
                                    },
                                },
                                children: vec![],
                                min_capacity: 1,
                                max_capacity: None,
                            },
                        ],
                        min_capacity: 1,
                        max_capacity: None,
                    },
                    DarkTree {
                        leaf: DarkTreeLeaf {
                            index: 21,
                            info: DarkLeaf {
                                data: 23,
                                parent_index: Some(22),
                                children_indexes: vec![17, 18, 20],
                            },
                        },
                        children: vec![
                            DarkTree {
                                leaf: DarkTreeLeaf {
                                    index: 17,
                                    info: DarkLeaf {
                                        data: 19,
                                        parent_index: Some(21),
                                        children_indexes: vec![15, 16],
                                    },
                                },
                                children: vec![
                                    DarkTree {
                                        leaf: DarkTreeLeaf {
                                            index: 15,
                                            info: DarkLeaf {
                                                data: 17,
                                                parent_index: Some(17),
                                                children_indexes: vec![],
                                            },
                                        },
                                        children: vec![],
                                        min_capacity: 1,
                                        max_capacity: None,
                                    },
                                    DarkTree {
                                        leaf: DarkTreeLeaf {
                                            index: 16,
                                            info: DarkLeaf {
                                                data: 18,
                                                parent_index: Some(17),
                                                children_indexes: vec![],
                                            },
                                        },
                                        children: vec![],
                                        min_capacity: 1,
                                        max_capacity: None,
                                    },
                                ],
                                min_capacity: 1,
                                max_capacity: None,
                            },
                            DarkTree {
                                leaf: DarkTreeLeaf {
                                    index: 18,
                                    info: DarkLeaf {
                                        data: 20,
                                        parent_index: Some(21),
                                        children_indexes: vec![],
                                    },
                                },
                                children: vec![],
                                min_capacity: 1,
                                max_capacity: None,
                            },
                            DarkTree {
                                leaf: DarkTreeLeaf {
                                    index: 20,
                                    info: DarkLeaf {
                                        data: 22,
                                        parent_index: Some(21),
                                        children_indexes: vec![19]
                                    },
                                },
                                children: vec![DarkTree {
                                    leaf: DarkTreeLeaf {
                                        index: 19,
                                        info: DarkLeaf {
                                            data: 21,
                                            parent_index: Some(20),
                                            children_indexes: vec![],
                                        },
                                    },
                                    children: vec![],
                                    min_capacity: 1,
                                    max_capacity: None,
                                },],
                                min_capacity: 1,
                                max_capacity: None,
                            },
                        ],
                        min_capacity: 1,
                        max_capacity: None,
                    },
                ],
                min_capacity: 1,
                max_capacity: None,
            }
        );

        let traversal_order: Vec<i32> = (2..25).collect();

        // Use [`DarkTree`] iterator to collect current
        // data, in order
        let nums: Vec<i32> = tree.iter().map(|x| x.info.data).collect();

        // Verify iterator collected the data in the expected
        // traversal order.
        assert_eq!(nums, traversal_order);

        // Thanks for reading
        Ok(())
    }

    #[test]
    fn test_darktree_min_capacity() -> DarkTreeResult<()> {
        // Generate a new [`DarkTree`] with min capacity 0
        let mut tree = DarkTree::new(0, vec![], Some(0), None);

        // Verify that min capacity was properly setup to 1
        assert_eq!(
            tree,
            DarkTree {
                leaf: DarkTreeLeaf {
                    index: 0,
                    info: DarkLeaf { data: 0, parent_index: None, children_indexes: vec![] },
                },
                children: vec![],
                min_capacity: 1,
                max_capacity: None
            }
        );

        // Verify that building it will succeed, as capacity
        // would have ben setup to 1
        assert!(tree.build().is_ok());

        // Generate a new [`DarkTree`] manually with
        // min capacity 0
        let mut tree = DarkTree {
            leaf: DarkTreeLeaf {
                index: 0,
                info: DarkLeaf { data: 0, parent_index: None, children_indexes: vec![] },
            },
            children: vec![],
            min_capacity: 0,
            max_capacity: None,
        };

        // Verify that building it will fail
        assert!(tree.build().is_err());

        // Thanks for reading
        Ok(())
    }

    #[test]
    fn test_darktree_max_capacity() -> DarkTreeResult<()> {
        // Generate a new [`DarkTree`] with max capacity 2
        let mut tree = DarkTree::new(0, vec![], None, Some(2));

        // Append a new node
        tree.append(DarkTree::new(1, vec![], None, None))?;

        // Try to append a new node
        assert!(tree.append(DarkTree::new(2, vec![], None, None)).is_err());

        // Verify tree builds
        tree.build()?;

        // Generate a new [`DarkTree`] with max capacity 2
        let mut new_tree = DarkTree::new(3, vec![], None, Some(2));

        // Append the previous tree as a new node
        new_tree.append(tree)?;

        // Check that max capacity has been exceeded
        assert!(new_tree.check_max_capacity().is_err());

        // Generate a new [`DarkTree`] manually with
        // max capacity 1
        let mut tree = DarkTree {
            leaf: DarkTreeLeaf {
                index: 0,
                info: DarkLeaf { data: 0, parent_index: None, children_indexes: vec![] },
            },
            children: vec![
                DarkTree {
                    leaf: DarkTreeLeaf {
                        index: 0,
                        info: DarkLeaf { data: 0, parent_index: None, children_indexes: vec![] },
                    },
                    children: vec![],
                    min_capacity: 1,
                    max_capacity: None,
                },
                DarkTree {
                    leaf: DarkTreeLeaf {
                        index: 0,
                        info: DarkLeaf { data: 0, parent_index: None, children_indexes: vec![] },
                    },
                    children: vec![],
                    min_capacity: 1,
                    max_capacity: None,
                },
                DarkTree {
                    leaf: DarkTreeLeaf {
                        index: 0,
                        info: DarkLeaf { data: 0, parent_index: None, children_indexes: vec![0] },
                    },
                    children: vec![],
                    min_capacity: 1,
                    max_capacity: None,
                },
            ],
            min_capacity: 1,
            max_capacity: Some(1),
        };

        // Verify that building it will fail
        assert!(tree.build().is_err());

        // Generate a new [`DarkTree`] with max capacity 0,
        // which is less that current min capacity 1
        let mut tree = DarkTree::new(0, vec![], None, Some(0));

        // Verify that building it will fail
        assert!(tree.build().is_err());

        // Thanks for reading
        Ok(())
    }

    #[test]
    fn test_darktree_flattened_vec() -> DarkTreeResult<()> {
        let (mut tree, traversal_order) = generate_tree()?;

        // Build the flattened vector
        let vec = tree.build_vec()?;

        // Verify vector integrity
        dark_leaf_vec_integrity_check(&vec, Some(23), Some(23), false)?;

        // Verify vector integrity will fail using different bounds:
        // 1. Leafs less that min capacity
        assert!(dark_leaf_vec_integrity_check(&vec, Some(24), None, false).is_err());
        // 2. Leafs more than max capacity
        assert!(dark_leaf_vec_integrity_check(&vec, None, Some(22), false).is_err());
        // 3. Max capacity less than min capacity
        assert!(dark_leaf_vec_integrity_check(&vec, Some(23), Some(22), false).is_err());

        // Loop the vector to verify it follows expected
        // traversal order.
        for (index, leaf) in vec.iter().enumerate() {
            assert_eq!(leaf.data, traversal_order[index]);
        }

        // Verify the tree is still intact
        let (new_tree, _) = generate_tree()?;
        assert_eq!(tree, new_tree);

        // Generate a new [`DarkLeaf`] vector manually,
        // corresponding to a [`DarkTree`] with a 2 children,
        // with erroneous indexes
        let vec = vec![
            DarkLeaf { data: 0, parent_index: Some(2), children_indexes: vec![] },
            DarkLeaf { data: 0, parent_index: Some(2), children_indexes: vec![] },
            DarkLeaf { data: 0, parent_index: None, children_indexes: vec![0, 2] },
        ];

        // Verify vector integrity will fail
        assert!(dark_leaf_vec_integrity_check(&vec, None, None, false).is_err());

        // Generate a new [`DarkLeaf`] vector manually,
        // corresponding to a [`DarkTree`] with out of bound parent index.
        let vec = vec![DarkLeaf { data: 0, parent_index: Some(2), children_indexes: vec![] }];

        // Verify vector integrity will fail
        assert!(dark_leaf_vec_integrity_check(&vec, None, None, false).is_err());

        // Generate a new [`DarkLeaf`] vector manually,
        // corresponding to a [`DarkTree`] with out of bound children indexes
        let vec = vec![DarkLeaf { data: 0, parent_index: None, children_indexes: vec![1] }];

        // Verify vector integrity will fail
        assert!(dark_leaf_vec_integrity_check(&vec, None, None, false).is_err());

        // Generate a new [`DarkLeaf`] vector manually,
        // corresponding to a [`DarkTree`] with duplicate children indexes
        let vec = vec![
            DarkLeaf { data: 0, parent_index: Some(2), children_indexes: vec![] },
            DarkLeaf { data: 0, parent_index: Some(2), children_indexes: vec![] },
            DarkLeaf { data: 0, parent_index: None, children_indexes: vec![1, 1, 2] },
        ];

        // Verify vector integrity will fail
        assert!(dark_leaf_vec_integrity_check(&vec, None, None, false).is_err());

        // Generate a new [`DarkLeaf`] vector manually,
        // corresponding to a [`DarkTree`] with children after parent
        let vec = vec![
            DarkLeaf { data: 0, parent_index: None, children_indexes: vec![1, 2] },
            DarkLeaf { data: 0, parent_index: Some(0), children_indexes: vec![] },
            DarkLeaf { data: 0, parent_index: Some(0), children_indexes: vec![] },
        ];

        // Verify vector integrity will fail
        assert!(dark_leaf_vec_integrity_check(&vec, None, None, false).is_err());

        // Generate a new [`DarkLeaf`] vector manually,
        // corresponding to a [`DarkTree`] with nothing indexed
        let vec = vec![
            DarkLeaf { data: 0, parent_index: None, children_indexes: vec![] },
            DarkLeaf { data: 0, parent_index: None, children_indexes: vec![] },
            DarkLeaf { data: 0, parent_index: None, children_indexes: vec![] },
        ];

        // Verify vector integrity will fail
        assert!(dark_leaf_vec_integrity_check(&vec, None, None, false).is_err());

        // Thanks for reading
        Ok(())
    }

    #[test]
    fn test_darktree_shifted_root_flattened_vec() -> DarkTreeResult<()> {
        let (mut tree, mut traversal_order) = generate_tree()?;

        // Shift root in traversal order
        let root = traversal_order.pop().unwrap();
        traversal_order.insert(0, root);

        // Build the flattened vector
        let vec = tree.build_shifted_root_vec()?;

        // Verify vector integrity
        dark_leaf_vec_integrity_check(&vec, Some(23), Some(23), true)?;

        // Verify vector integrity will fail using different bounds:
        // 1. Leafs less that min capacity
        assert!(dark_leaf_vec_integrity_check(&vec, Some(24), None, true).is_err());
        // 2. Leafs more than max capacity
        assert!(dark_leaf_vec_integrity_check(&vec, None, Some(22), true).is_err());
        // 3. Max capacity less than min capacity
        assert!(dark_leaf_vec_integrity_check(&vec, Some(23), Some(22), true).is_err());

        // Loop the vector to verify it follows expected
        // traversal order.
        for (index, leaf) in vec.iter().enumerate() {
            assert_eq!(leaf.data, traversal_order[index]);
        }

        // Verify the tree is still intact
        let (new_tree, _) = generate_tree()?;
        assert_eq!(tree, new_tree);

        // Generate a new [`DarkLeaf`] vector manually,
        // corresponding to a [`DarkTree`] with a 2 children,
        // with erroneous indexes
        let vec = vec![
            DarkLeaf { data: 0, parent_index: None, children_indexes: vec![0, 2] },
            DarkLeaf { data: 0, parent_index: Some(0), children_indexes: vec![] },
            DarkLeaf { data: 0, parent_index: Some(0), children_indexes: vec![] },
        ];

        // Verify vector integrity will fail
        assert!(dark_leaf_vec_integrity_check(&vec, None, None, true).is_err());

        // Generate a new [`DarkLeaf`] vector manually,
        // corresponding to a [`DarkTree`] with out of bound parent index.
        let vec = vec![DarkLeaf { data: 0, parent_index: Some(2), children_indexes: vec![] }];

        // Verify vector integrity will fail
        assert!(dark_leaf_vec_integrity_check(&vec, None, None, true).is_err());

        // Generate a new [`DarkLeaf`] vector manually,
        // corresponding to a [`DarkTree`] with out of bound children indexes
        let vec = vec![DarkLeaf { data: 0, parent_index: None, children_indexes: vec![1] }];

        // Verify vector integrity will fail
        assert!(dark_leaf_vec_integrity_check(&vec, None, None, true).is_err());

        // Generate a new [`DarkLeaf`] vector manually,
        // corresponding to a [`DarkTree`] with duplicate children indexes
        let vec = vec![
            DarkLeaf { data: 0, parent_index: None, children_indexes: vec![1, 1, 2] },
            DarkLeaf { data: 0, parent_index: Some(0), children_indexes: vec![] },
            DarkLeaf { data: 0, parent_index: Some(0), children_indexes: vec![] },
        ];

        // Verify vector integrity will fail
        assert!(dark_leaf_vec_integrity_check(&vec, None, None, true).is_err());

        // Generate a new [`DarkLeaf`] vector manually,
        // corresponding to a [`DarkTree`] with children before parent
        let vec = vec![
            DarkLeaf { data: 0, parent_index: Some(2), children_indexes: vec![] },
            DarkLeaf { data: 0, parent_index: Some(2), children_indexes: vec![] },
            DarkLeaf { data: 0, parent_index: None, children_indexes: vec![0, 1] },
        ];

        // Verify vector integrity will fail
        assert!(dark_leaf_vec_integrity_check(&vec, None, None, true).is_err());

        // Generate a new [`DarkLeaf`] vector manually,
        // corresponding to a [`DarkTree`] with nothing indexed
        let vec = vec![
            DarkLeaf { data: 0, parent_index: None, children_indexes: vec![] },
            DarkLeaf { data: 0, parent_index: None, children_indexes: vec![] },
            DarkLeaf { data: 0, parent_index: None, children_indexes: vec![] },
        ];

        // Verify vector integrity will fail
        assert!(dark_leaf_vec_integrity_check(&vec, None, None, true).is_err());

        // Thanks for reading
        Ok(())
    }
}
