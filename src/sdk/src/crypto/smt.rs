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

use core::marker::PhantomData;
use std::collections::{BTreeMap, BTreeSet};

use halo2_gadgets::poseidon::{
    primitives as poseidon,
    primitives::{ConstantLength, P128Pow5T3, Spec},
};
use pasta_curves::group::ff::{FromUniformBytes, WithSmallOrderMulGroup};

use crate::error::{ContractError, GenericResult};

pub trait FieldHasher<F: WithSmallOrderMulGroup<3> + Ord, const L: usize> {
    fn hash(&self, inputs: [F; L]) -> GenericResult<F>;
    fn hasher() -> Self;
}

#[derive(Debug, Clone)]
pub struct Poseidon<F: WithSmallOrderMulGroup<3> + Ord, const L: usize>(PhantomData<F>);

impl<F: WithSmallOrderMulGroup<3> + Ord, const L: usize> Poseidon<F, L> {
    pub fn new() -> Self {
        Poseidon(PhantomData)
    }
}

impl<F: WithSmallOrderMulGroup<3> + Ord, const L: usize> Default for Poseidon<F, L> {
    fn default() -> Self {
        Self::new()
    }
}

impl<F: WithSmallOrderMulGroup<3> + Ord, const L: usize> FieldHasher<F, L> for Poseidon<F, L>
where
    P128Pow5T3: Spec<F, 3, 2>,
{
    fn hash(&self, inputs: [F; L]) -> GenericResult<F> {
        Ok(poseidon::Hash::<_, P128Pow5T3, ConstantLength<L>, 3, 2>::init().hash(inputs))
    }

    fn hasher() -> Self {
        Poseidon(PhantomData)
    }
}

/// The Path struct.
///
/// The path contains a sequence of sibling nodes that make up a Merkle proof.
/// Each pair is used to identify whether an incremental Merkle root construction
/// is valid at each intermediate step.
#[derive(Debug, Copy, Clone)]
pub struct Path<F: WithSmallOrderMulGroup<3> + Ord, H: FieldHasher<F, 2>, const N: usize> {
    /// The path represented as a sequence of sibling pairs.
    pub path: [(F, F); N],
    /// The phantom hasher type used to reconstruct the Merkle root.
    pub marker: PhantomData<H>,
}

impl<F: WithSmallOrderMulGroup<3> + Ord, H: FieldHasher<F, 2>, const N: usize> Path<F, H, N> {
    /// Assumes leaf contains leaf-level data, i.e. hashes of secrets stored on
    /// leaf-level.
    pub fn calculate_root(&self, leaf: &F, hasher: &H) -> GenericResult<F> {
        if *leaf != self.path[0].0 && *leaf != self.path[0].1 {
            return Err(ContractError::SmtInvalidLeaf)
        }

        let mut prev = *leaf;
        // Check levels between leaf level and root
        for (left_hash, right_hash) in &self.path {
            if &prev != left_hash && &prev != right_hash {
                return Err(ContractError::SmtInvalidPathNodes)
            }
            prev = hasher.hash([*left_hash, *right_hash])?;
        }

        Ok(prev)
    }

    /// Takes in an expected `root_hash` and leaf-level data (i.e. hashes of secrets)
    /// for a leaf and checks that the leaf belongs to a tree having the expected hash.
    pub fn check_membership(&self, root_hash: &F, leaf: &F, hasher: &H) -> GenericResult<bool> {
        let root = self.calculate_root(leaf, hasher)?;
        Ok(root == *root_hash)
    }

    /// Given leaf data, determine what the index of this leaf must be in the
    /// Merkle tree it belongs to. Before doing so, check that the leaf does
    /// indeed belong to a tree with the given `root_hash`.
    pub fn get_index(&self, root_hash: &F, leaf: &F, hasher: &H) -> GenericResult<F> {
        if !self.check_membership(root_hash, leaf, hasher)? {
            return Err(ContractError::SmtInvalidLeaf)
        }

        let mut prev = *leaf;
        let mut index = F::ZERO;
        let mut twopower = F::ONE;
        // Check levels between leaf level and root
        for (left_hash, right_hash) in &self.path {
            // Check if the previous hash is for a left or right ndoe
            if &prev != left_hash {
                index += twopower;
            }

            twopower = twopower + twopower;
            prev = hasher.hash([*left_hash, *right_hash])?;
        }

        Ok(index)
    }
}

/// The Sparse Merkle Tree struct.
///
/// SMT stores a set of leaves represented in a map and a set of empty
/// hashes that it uses to represent the sparse areas of the tree.
pub struct SparseMerkleTree<
    F: WithSmallOrderMulGroup<3> + Ord,
    H: FieldHasher<F, 2>,
    const N: usize,
> {
    /// A map from leaf indices to leaf data stored as field elements.
    pub tree: BTreeMap<u64, F>,
    /// An array of default hashes hashed with themselves `N` times.
    empty_hashes: [F; N],
    /// The phantom hasher type used to build the Merkle tree.
    marker: PhantomData<H>,
}

impl<
        F: WithSmallOrderMulGroup<3> + Ord + FromUniformBytes<64>,
        H: FieldHasher<F, 2>,
        const N: usize,
    > SparseMerkleTree<F, H, N>
{
    /// Creates a new SMT from a map of indices to field elements.
    pub fn new(
        leaves: &BTreeMap<u32, F>,
        hasher: &H,
        empty_leaf: &[u8; 64],
    ) -> GenericResult<Self> {
        // Ensure the tree can hold this many leaves
        let last_level_size = leaves.len().next_power_of_two();
        let tree_size = 2 * last_level_size - 1;
        let tree_height = tree_height(tree_size as u64);
        assert!(tree_height <= N as u32);

        // Initialize the Merkle tree
        let tree = BTreeMap::new();
        let empty_hashes = gen_empty_hashes(hasher, empty_leaf)?;

        let mut smt = SparseMerkleTree::<F, H, N> { tree, empty_hashes, marker: PhantomData };

        smt.insert_batch(leaves, hasher)?;

        Ok(smt)
    }

    /// Creates a new SMT from an array of field elements.
    pub fn new_sequential(leaves: &[F], hasher: &H, empty_leaf: &[u8; 64]) -> GenericResult<Self> {
        let pairs: BTreeMap<u32, F> =
            leaves.iter().enumerate().map(|(i, l)| (i as u32, *l)).collect();

        let smt = Self::new(&pairs, hasher, empty_leaf)?;

        Ok(smt)
    }

    /// Takes a batch of field elements, inserts these hashes into the tree,
    /// and updates the Merkle root.
    pub fn insert_batch(&mut self, leaves: &BTreeMap<u32, F>, hasher: &H) -> GenericResult<()> {
        let last_level_index: u64 = (1u64 << N) - 1;

        let mut level_idxs: BTreeSet<u64> = BTreeSet::new();
        for (i, leaf) in leaves {
            let true_index = last_level_index + (*i as u64);
            self.tree.insert(true_index, *leaf);
            level_idxs.insert(parent(true_index).unwrap());
        }

        for level in 0..N {
            let mut new_idxs: BTreeSet<u64> = BTreeSet::new();
            let empty_hash = self.empty_hashes[level];
            for i in level_idxs {
                let left_index = left_child(i);
                let right_index = right_child(i);
                let left = self.tree.get(&left_index).unwrap_or(&empty_hash);
                let right = self.tree.get(&right_index).unwrap_or(&empty_hash);
                self.tree.insert(i, hasher.hash([*left, *right])?);

                let parent = match parent(i) {
                    Some(i) => i,
                    None => break,
                };

                new_idxs.insert(parent);
            }

            level_idxs = new_idxs;
        }

        Ok(())
    }

    /// Returns the Merkle tree root.
    pub fn root(&self) -> F {
        self.tree.get(&0).cloned().unwrap_or(*self.empty_hashes.last().unwrap())
    }

    /// Give the path leading from the leaf at `index` up to the root. This is
    /// a "proof" in the sense of "valid path in a Merkle tree", not a ZK argument.
    pub fn generate_membership_proof(&self, index: u64) -> Path<F, H, N> {
        let mut path = [(F::ZERO, F::ZERO); N];

        let tree_index = convert_index_to_last_level(index, N);

        // Iterate from the leaf up to the root, storing all intermediate hash values.
        let mut current_node = tree_index;
        let mut level = 0;
        while !is_root(current_node) {
            let sibling_node = sibling(current_node).unwrap();

            let empty_hash = &self.empty_hashes[level];

            let current = self.tree.get(&current_node).cloned().unwrap_or(*empty_hash);
            let sibling = self.tree.get(&sibling_node).cloned().unwrap_or(*empty_hash);

            if is_left_child(current_node) {
                path[level] = (current, sibling);
            } else {
                path[level] = (sibling, current);
            }

            current_node = parent(current_node).unwrap();
            level += 1;
        }

        Path { path, marker: PhantomData }
    }
}

/// A function to generate empty hashes with a given `default_leaf`.
///
/// Given a `FieldHasher`, generate a list of `N` hashes consisting of the
/// `default_leaf` hashed with itself and repeated `N` times with the
/// intermediate results. These are used to initialize the sparse portion
/// of the SMT.
pub fn gen_empty_hashes<
    F: WithSmallOrderMulGroup<3> + Ord + FromUniformBytes<64>,
    H: FieldHasher<F, 2>,
    const N: usize,
>(
    hasher: &H,
    default_leaf: &[u8; 64],
) -> GenericResult<[F; N]> {
    let mut empty_hashes = [F::ZERO; N];

    let mut empty_hash = F::from_uniform_bytes(default_leaf);
    for item in empty_hashes.iter_mut().take(N) {
        *item = empty_hash;
        empty_hash = hasher.hash([empty_hash, empty_hash])?;
    }

    Ok(empty_hashes)
}

fn convert_index_to_last_level(index: u64, height: usize) -> u64 {
    index + (1u64 << height) - 1
}

/// Returns the log2 value of the given number.
#[inline]
fn log2(x: u64) -> u32 {
    if x == 0 {
        0
    } else if x.is_power_of_two() {
        1usize.leading_zeros() - x.leading_zeros()
    } else {
        0usize.leading_zeros() - x.leading_zeros()
    }
}

/// Returns the index of the left child, given an index.
#[inline]
fn left_child(index: u64) -> u64 {
    2 * index + 1
}

/// Returns the index of the right child, given an index.
#[inline]
fn right_child(index: u64) -> u64 {
    2 * index + 2
}

/// Returns true iff the given index represents a left child.
#[inline]
fn is_left_child(index: u64) -> bool {
    index % 2 == 1
}

/// Returns the index of the parent, given an index.
#[inline]
fn parent(index: u64) -> Option<u64> {
    if index > 0 {
        Some((index - 1) >> 1)
    } else {
        None
    }
}

/// Returns the index of the sibling, given an index.
#[inline]
fn sibling(index: u64) -> Option<u64> {
    if index == 0 {
        None
    } else if is_left_child(index) {
        Some(index + 1)
    } else {
        Some(index - 1)
    }
}

/// Returns the height of the tree, given the size of the tree.
#[inline]
fn tree_height(tree_size: u64) -> u32 {
    log2(tree_size)
}

/// Returns true iff the index represents the Merkle root.
#[inline]
fn is_root(index: u64) -> bool {
    index == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use halo2_proofs::arithmetic::Field;
    use pasta_curves::Fp;
    use rand::rngs::OsRng;

    /// Helper to change leaves array to BTreeMap and then create SMT.
    fn create_merkle_tree<
        F: WithSmallOrderMulGroup<3> + Ord,
        H: FieldHasher<F, 2>,
        const N: usize,
    >(
        hasher: H,
        leaves: &[F],
        default_leaf: &[u8; 64],
    ) -> SparseMerkleTree<F, H, N>
    where
        F: FromUniformBytes<64>,
    {
        SparseMerkleTree::<F, H, N>::new_sequential(leaves, &hasher, default_leaf).unwrap()
    }

    #[test]
    fn poseidon_smt() {
        let poseidon = Poseidon::<Fp, 2>::new();
        let default_leaf = [0u8; 64];
        let leaves = [Fp::random(&mut OsRng), Fp::random(&mut OsRng), Fp::random(&mut OsRng)];
        const HEIGHT: usize = 3;

        let smt = create_merkle_tree::<Fp, Poseidon<Fp, 2>, HEIGHT>(
            poseidon.clone(),
            &leaves,
            &default_leaf,
        );

        let root = smt.root();

        let empty_hashes =
            gen_empty_hashes::<Fp, Poseidon<Fp, 2>, HEIGHT>(&poseidon, &default_leaf).unwrap();

        let hash1 = leaves[0];
        let hash2 = leaves[1];
        let hash3 = leaves[2];

        let hash12 = poseidon.hash([hash1, hash2]).unwrap();
        let hash34 = poseidon.hash([hash3, empty_hashes[0]]).unwrap();

        let hash1234 = poseidon.hash([hash12, hash34]).unwrap();
        let calc_root = poseidon.hash([hash1234, empty_hashes[2]]).unwrap();

        assert_eq!(root, calc_root);
    }

    #[test]
    fn poseidon_smt_incl_proof() {
        let poseidon = Poseidon::<Fp, 2>::new();
        let default_leaf = [0u8; 64];
        let leaves = [Fp::random(&mut OsRng), Fp::random(&mut OsRng), Fp::random(&mut OsRng)];
        const HEIGHT: usize = 3;

        let smt = create_merkle_tree::<Fp, Poseidon<Fp, 2>, HEIGHT>(
            poseidon.clone(),
            &leaves,
            &default_leaf,
        );

        let proof = smt.generate_membership_proof(0);
        let res = proof.check_membership(&smt.root(), &leaves[0], &poseidon).unwrap();
        assert!(res)
    }
}
