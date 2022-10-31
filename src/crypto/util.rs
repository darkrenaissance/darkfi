/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use blake2b_simd::Params;
use darkfi_sdk::crypto::constants::{
    fixed_bases::{
        VALUE_COMMITMENT_PERSONALIZATION, VALUE_COMMITMENT_R_BYTES, VALUE_COMMITMENT_V_BYTES,
    },
    util::gen_const_array,
    NullifierK,
};
use halo2_gadgets::{ecc::chip::FixedPoint, poseidon::primitives as poseidon};
use pasta_curves::{
    arithmetic::{CurveExt, FieldExt},
    group::ff::PrimeField,
    pallas,
};

use super::types::*;

pub fn hash_to_scalar(persona: &[u8], a: &[u8], b: &[u8]) -> pallas::Scalar {
    let mut hasher = Params::new().hash_length(64).personal(persona).to_state();
    hasher.update(a);
    hasher.update(b);
    let ret = hasher.finalize();
    pallas::Scalar::from_bytes_wide(ret.as_array())
}

/// Pedersen commitment for a full-width base field element.
#[allow(non_snake_case)]
pub fn pedersen_commitment_base(value: pallas::Base, blind: DrkValueBlind) -> DrkValueCommit {
    let hasher = DrkValueCommit::hash_to_curve(VALUE_COMMITMENT_PERSONALIZATION);
    let V = NullifierK.generator();
    let R = hasher(&VALUE_COMMITMENT_R_BYTES);

    V * mod_r_p(value) + R * blind
}

/// Pedersen commitment for a 64-bit value, in the base field.
#[allow(non_snake_case)]
pub fn pedersen_commitment_u64(value: u64, blind: DrkValueBlind) -> DrkValueCommit {
    let hasher = DrkValueCommit::hash_to_curve(VALUE_COMMITMENT_PERSONALIZATION);
    let V = hasher(&VALUE_COMMITMENT_V_BYTES);
    let R = hasher(&VALUE_COMMITMENT_R_BYTES);

    V * mod_r_p(DrkValue::from(value)) + R * blind
}

/// Simplified wrapper for poseidon hash function.
pub fn poseidon_hash<const N: usize>(messages: [pallas::Base; N]) -> pallas::Base {
    poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<N>, 3, 2>::init()
        .hash(messages)
}

/// Converts from pallas::Base to pallas::Scalar (aka $x \pmod{r_\mathbb{P}}$).
///
/// This requires no modular reduction because Pallas' base field is smaller than its
/// scalar field.
pub fn mod_r_p(x: pallas::Base) -> pallas::Scalar {
    pallas::Scalar::from_repr(x.to_repr()).unwrap()
}

/// The sequence of bits representing a u64 in little-endian order.
///
/// # Panics
///
/// Panics if the expected length of the sequence `NUM_BITS` exceeds
/// 64.
pub fn i2lebsp<const NUM_BITS: usize>(int: u64) -> [bool; NUM_BITS] {
    assert!(NUM_BITS <= 64);
    gen_const_array(|mask: usize| (int & (1 << mask)) != 0)
}
