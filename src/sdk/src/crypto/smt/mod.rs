/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
 *
 * Copyright (C) 2021 Webb Technologies Inc.
 * Copyright (c) zkMove Authors
 * SPDX-License-Identifier: Apache-2.0
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

//! This file provides a native implementation of the Sparse Merkle tree data
//! structure.
//!
//! A Sparse Merkle tree is a type of Merkle tree, but it is much easier to
//! prove non-membership in a sparse Merkle tree than in an arbitrary Merkle
//! tree. For an explanation of sparse Merkle trees, see:
//! `<https://medium.com/@kelvinfichter/whats-a-sparse-merkle-tree-acda70aeb837>`
//!
//! In this file we define the `Path` and `SparseMerkleTree` structs.
//! These depend on your choice of a prime field F, a field hasher over F
//! (any hash function that maps F^2 to F will do, e.g. the poseidon hash
//! function of width 3 where an input of zero is used for padding), and the
//! height N of the sparse Merkle tree.
//!
//! The path corresponding to a given leaf node is stored as an N-tuple of pairs
//! of field elements. Each pair consists of a node lying on the path from the
//! leaf node to the root, and that node's sibling.  For example, suppose
//! ```text
//!           a
//!         /   \
//!        b     c
//!       / \   / \
//!      d   e f   g
//! ```
//! is our Sparse Merkle tree, and `a` through `g` are field elements stored at
//! the nodes. Then the merkle proof path `e-b-a` from leaf `e` to root `a` is
//! stored as `[(d,e), (b,c)]`
//!
//! # Terminology
//!
//! * **level** - the depth in the tree. Type: `u32`
//! * **location** - a `(level, position)` tuple
//! * **position** - the leaf index, or equivalently the binary direction through the tree
//!   with type `F`.
//! * **index** - the internal index used in the DB which is `BigUint`. Leaf node indexes are
//!   calculated as `leaf_idx = final_level_start_idx + position`.
//! * **node** - either the leaf values or parent nodes `hash(left, right)`.

use num_bigint::BigUint;
use std::collections::HashMap;
// Only used for the type aliases below
use pasta_curves::pallas;

use crate::error::ContractResult;
use util::{FieldElement, FieldHasher};

mod empty;
pub use empty::EMPTY_NODES_FP;

#[cfg(test)]
mod test;

pub mod util;
pub use util::Poseidon;

#[cfg(feature = "wasm")]
pub mod wasmdb;

// Bit size for Fp (and Fq)
pub const SMT_FP_DEPTH: usize = 255;
pub type PoseidonFp = Poseidon<pallas::Base, 2>;
pub type MemoryStorageFp = MemoryStorage<pallas::Base>;
pub type SmtMemoryFp = SparseMerkleTree<
    'static,
    SMT_FP_DEPTH,
    { SMT_FP_DEPTH + 1 },
    pallas::Base,
    PoseidonFp,
    MemoryStorageFp,
>;
pub type PathFp = Path<SMT_FP_DEPTH, pallas::Base, PoseidonFp>;

/// Pluggable storage backend for the SMT.
/// Has a minimal interface to put, get, and delete objects from the store.
pub trait StorageAdapter {
    type Value;

    fn put(&mut self, key: BigUint, value: Self::Value) -> ContractResult;
    fn get(&self, key: &BigUint) -> Option<Self::Value>;
    fn del(&mut self, key: &BigUint) -> ContractResult;
}

/// An in-memory storage, useful for unit tests and smaller trees.
#[derive(Default, Clone)]
pub struct MemoryStorage<F: FieldElement> {
    pub tree: HashMap<BigUint, F>,
}

impl<F: FieldElement> MemoryStorage<F> {
    pub fn new() -> Self {
        Self { tree: HashMap::new() }
    }
}

impl<F: FieldElement> StorageAdapter for MemoryStorage<F> {
    type Value = F;

    fn put(&mut self, key: BigUint, value: F) -> ContractResult {
        self.tree.insert(key, value);
        Ok(())
    }

    fn get(&self, key: &BigUint) -> Option<F> {
        self.tree.get(key).copied()
    }

    fn del(&mut self, key: &BigUint) -> ContractResult {
        self.tree.remove(key);
        Ok(())
    }
}

/// The Sparse Merkle Tree struct.
///
/// SMT stores a set of leaves represented in a map and a set of empty
/// hashes that it uses to represent the sparse areas of the tree.
///
/// The trait param `N` is the depth of the tree. A tree with a depth of `N`
/// will have `N + 1` levels.
#[derive(Debug, Clone)]
pub struct SparseMerkleTree<
    'a,
    const N: usize,
    // M = N + 1
    const M: usize,
    F: FieldElement,
    H: FieldHasher<F, 2>,
    S: StorageAdapter<Value = F>,
> {
    /// A map from leaf indices to leaf data stored as field elements.
    store: S,
    /// The hasher used to build the Merkle tree.
    hasher: H,
    /// An array of empty hashes hashed with themselves `N` times.
    empty_nodes: &'a [F; M],
}

impl<
        'a,
        const N: usize,
        const M: usize,
        F: FieldElement,
        H: FieldHasher<F, 2>,
        S: StorageAdapter<Value = F>,
    > SparseMerkleTree<'a, N, M, F, H, S>
{
    /// Creates a new SMT
    pub fn new(store: S, hasher: H, empty_nodes: &'a [F; M]) -> Self {
        assert_eq!(M, N + 1);
        Self { store, hasher, empty_nodes }
    }

    /// Takes a batch of field elements, inserts these hashes into the tree,
    /// and updates the Merkle root.
    pub fn insert_batch(&mut self, leaves: Vec<(F, F)>) -> ContractResult {
        if leaves.is_empty() {
            return Ok(())
        }

        // Nodes that need recalculating
        let mut dirty_idxs = Vec::new();
        for (pos, leaf) in leaves {
            let idx = util::leaf_pos_to_index::<N, _>(&pos);
            self.put_node(idx.clone(), leaf)?;

            // Mark node parent as dirty
            let parent_idx = util::parent(&idx).unwrap();
            dirty_idxs.push(parent_idx);
        }

        self.recompute_tree(&mut dirty_idxs)?;

        Ok(())
    }

    pub fn remove_leaves(&mut self, leaves: Vec<(F, F)>) -> ContractResult {
        if leaves.is_empty() {
            return Ok(())
        }

        let mut dirty_idxs = Vec::new();
        for (pos, _leaf) in leaves {
            let idx = util::leaf_pos_to_index::<N, _>(&pos);
            self.remove_node(&idx)?;

            // Mark node parent as dirty
            let parent_idx = util::parent(&idx).unwrap();
            dirty_idxs.push(parent_idx);
        }

        self.recompute_tree(&mut dirty_idxs)?;

        Ok(())
    }

    /// Returns the Merkle tree root.
    pub fn root(&self) -> F {
        self.get_node(&BigUint::from(0u32))
    }

    /// Recomputes the Merkle tree depth first from the bottom of the tree
    fn recompute_tree(&mut self, dirty_idxs: &mut Vec<BigUint>) -> ContractResult {
        for _ in 0..N + 1 {
            let mut new_dirty_idxs = vec![];

            for idx in &mut *dirty_idxs {
                let left_idx = util::left_child(idx);
                let right_idx = util::right_child(idx);
                let left = self.get_node(&left_idx);
                let right = self.get_node(&right_idx);
                // Recalclate the node
                let node = self.hasher.hash([left, right]);
                self.put_node(idx.clone(), node)?;

                // Add this node's parent to the update list
                let parent_idx = match util::parent(idx) {
                    Some(idx) => idx,
                    // We are at the root node so no parents exist
                    None => break,
                };

                new_dirty_idxs.push(parent_idx);
            }

            *dirty_idxs = new_dirty_idxs;
        }

        Ok(())
    }

    /// Give the path leading from the leaf at `index` up to the root. This is
    /// a "proof" in the sense of "valid path in a Merkle tree", not a ZK argument.
    pub fn prove_membership(&self, pos: &F) -> Path<N, F, H> {
        let mut path = [F::ZERO; N];
        let leaf_idx = util::leaf_pos_to_index::<N, _>(pos);

        let mut current_idx = leaf_idx;
        // Depth first from the bottom of the tree
        for lvl in (0..N).rev() {
            let sibling_idx = util::sibling(&current_idx).unwrap();
            let sibling_node = self.get_node(&sibling_idx);
            path[lvl] = sibling_node;

            // Now move to the parent
            current_idx = util::parent(&current_idx).unwrap();
        }

        Path { path, hasher: self.hasher.clone() }
    }

    /// Fast lookup for leaf. The SMT can be used as a generic container for
    /// objects with very little overhead using this method.
    pub fn get_leaf(&self, pos: &F) -> F {
        let leaf_idx = util::leaf_pos_to_index::<N, _>(pos);
        self.get_node(&leaf_idx)
    }

    fn get_node(&self, idx: &BigUint) -> F {
        let lvl = util::log2(idx);
        let empty_node = self.empty_nodes[lvl as usize];
        self.store.get(idx).unwrap_or(empty_node)
    }

    fn put_node(&mut self, key: BigUint, value: F) -> ContractResult {
        self.store.put(key, value)
    }

    fn remove_node(&mut self, key: &BigUint) -> ContractResult {
        self.store.del(key)
    }
}

/// The path contains a sequence of sibling nodes that make up a Merkle proof.
/// Each sibling node is used to identify whether the merkle root construction
/// is valid at the root.
pub struct Path<const N: usize, F: FieldElement, H: FieldHasher<F, 2>> {
    /// Path from leaf to root. It is a list of sibling nodes.
    /// It does not contain the root node.
    /// Similar to other conventions here, the list starts higher in the tree
    /// and goes down. So when iterating we start from the end.
    pub path: [F; N],
    hasher: H,
}

impl<const N: usize, F: FieldElement, H: FieldHasher<F, 2>> Path<N, F, H> {
    pub fn verify(&self, root: &F, leaf: &F, pos: &F) -> bool {
        let pos = pos.as_biguint();
        assert!(pos.bits() as usize <= N);

        let mut current_node = *leaf;
        for i in (0..N).rev() {
            let sibling_node = self.path[i];

            let is_right = pos.bit((N - 1 - i) as u64);
            let (left, right) =
                if is_right { (sibling_node, current_node) } else { (current_node, sibling_node) };
            //println!("is_right: {}", is_right);
            //println!("left: {:?}, right: {:?}", left, right);
            //println!("current_node: {:?}", current_node);

            current_node = self.hasher.hash([left, right]);
        }

        current_node == *root
    }
}

/// A function to generate empty hashes with a given `default_leaf`.
///
/// Given a `FieldHasher`, generate a list of `N` hashes consisting of the
/// `default_leaf` hashed with itself and repeated `N` times with the
/// intermediate results. These are used to initialize the sparse portion
/// of the SMT.
///
/// Ordering is depth-wise starting from root going down.
pub fn gen_empty_nodes<const M: usize, F: FieldElement, H: FieldHasher<F, 2>>(
    hasher: &H,
    empty_leaf: F,
) -> [F; M] {
    let mut empty_nodes = [F::ZERO; M];
    let mut empty_node = empty_leaf;

    for item in empty_nodes.iter_mut().rev() {
        *item = empty_node;
        empty_node = hasher.hash([empty_node, empty_node]);
    }

    empty_nodes
}
