/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::marker::PhantomData;

use halo2_gadgets::poseidon::{
    primitives as poseidon,
    primitives::{ConstantLength, P128Pow5T3, Spec},
};
use num_bigint::BigUint;
use pasta_curves::group::ff::{PrimeField, WithSmallOrderMulGroup};

pub trait FieldElement: WithSmallOrderMulGroup<3> + Ord + PrimeField {
    fn as_biguint(&self) -> BigUint;
}

impl FieldElement for pasta_curves::Fp {
    fn as_biguint(&self) -> BigUint {
        let repr = self.to_repr();
        BigUint::from_bytes_le(&repr)
    }
}

impl FieldElement for pasta_curves::Fq {
    fn as_biguint(&self) -> BigUint {
        let repr = self.to_repr();
        BigUint::from_bytes_le(&repr)
    }
}

pub trait FieldHasher<F: WithSmallOrderMulGroup<3> + Ord, const L: usize>: Clone {
    fn hash(&self, inputs: [F; L]) -> F;
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
    fn hash(&self, inputs: [F; L]) -> F {
        poseidon::Hash::<_, P128Pow5T3, ConstantLength<L>, 3, 2>::init().hash(inputs)
    }

    fn hasher() -> Self {
        Poseidon(PhantomData)
    }
}

#[inline]
/// Converts a leaf position to the internal BigUint index for storage.
pub(super) fn leaf_pos_to_index<const N: usize, F: FieldElement>(pos: &F) -> BigUint {
    // Starting index for the last level
    // 2^N - 1
    let final_level_index = (BigUint::from(1u32) << (N as u64)) - 1u32;

    final_level_index + pos.as_biguint()
}

/// Returns the log2 value of the given number. Used for converting the index to the level.
#[inline]
pub(super) fn log2(x: &BigUint) -> u64 {
    (x + 1u32).bits() - 1
}

/// Returns the index of the left child, given an index.
#[inline]
pub(super) fn left_child(index: &BigUint) -> BigUint {
    2u32 * index + 1u32
}

/// Returns the index of the right child, given an index.
#[inline]
pub(super) fn right_child(index: &BigUint) -> BigUint {
    2u32 * index + 2u32
}

/// Returns true iff the given index represents a left child.
#[inline]
pub(super) fn is_left_child(index: &BigUint) -> bool {
    // Any simple way to convert the (index % 2) into a u32 rather
    // than converting 1 into a BigUint?
    index % 2u32 == 1u32.into()
}

/// Returns the index of the parent, given an index.
#[inline]
pub(super) fn parent(index: &BigUint) -> Option<BigUint> {
    if *index > 0u32.into() {
        Some((index - 1u32) >> 1)
    } else {
        None
    }
}

/// Returns the index of the sibling, given an index.
#[inline]
pub(super) fn sibling(index: &BigUint) -> Option<BigUint> {
    if *index == 0u32.into() {
        None
    } else if is_left_child(index) {
        Some(index + 1u32)
    } else {
        Some(index - 1u32)
    }
}
