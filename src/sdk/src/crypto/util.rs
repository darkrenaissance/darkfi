/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use darkfi_serial::ReadExt;
use halo2_gadgets::poseidon::primitives as poseidon;
use pasta_curves::{
    group::ff::{FromUniformBytes, PrimeField},
    pallas,
};
use std::io::Cursor;
use subtle::CtOption;

/// Hash `a` and `b` together with a prefix `persona` and return a `pallas::Scalar`
/// element from the digest.
pub fn hash_to_scalar(persona: &[u8], a: &[u8], b: &[u8]) -> pallas::Scalar {
    let mut hasher = blake2b_simd::Params::new().hash_length(64).personal(persona).to_state();
    hasher.update(a);
    hasher.update(b);
    let ret = hasher.finalize();
    pallas::Scalar::from_uniform_bytes(ret.as_array())
}

/// Converts from pallas::Base to pallas::Scalar (aka $x \pmod{r_\mathbb{P}}$).
///
/// This requires no modular reduction because Pallas' base field is smaller than its
/// scalar field.
pub fn fp_mod_fv(val: pallas::Base) -> pallas::Scalar {
    pallas::Scalar::from_repr(val.to_repr()).unwrap()
}

/// Converts from pallas::Scalar to pallas::Base (aka $x \pmod{r_\mathbb{P}}$).
///
/// This call is unsafe and liable to fail. Use with caution.
/// The Pallas scalar field is bigger than the field we're converting to here.
pub fn fv_mod_fp_unsafe(val: pallas::Scalar) -> CtOption<pallas::Base> {
    pallas::Base::from_repr(val.to_repr())
}

/// Wrapper around poseidon in `halo2_gadgets`
pub fn poseidon_hash<const N: usize>(messages: [pallas::Base; N]) -> pallas::Base {
    // TODO: it's possible to make this function simply take a slice, by using the lower level
    // sponge defined in halo2 lib. Simply look how the function hash() is defined.
    // Why is this needed? Simply put we are often working with dynamic data such as Python
    // or with other interpreted environments. We don't always know the length of input data
    // at compile time.
    poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<N>, 3, 2>::init()
        .hash(messages)
}

pub fn fp_to_u64(value: pallas::Base) -> Option<u64> {
    let repr = value.to_repr();
    if !repr[8..].iter().all(|&b| b == 0u8) {
        return None
    }
    let mut cur = Cursor::new(&repr[0..8]);
    let uint = ReadExt::read_u64(&mut cur).ok()?;
    Some(uint)
}

#[test]
fn test_fp_to_u64() {
    use super::pasta_prelude::Field;

    let fp = pallas::Base::from(u64::MAX);
    assert_eq!(fp_to_u64(fp), Some(u64::MAX));
    assert_eq!(fp_to_u64(fp + pallas::Base::ONE), None);
}
