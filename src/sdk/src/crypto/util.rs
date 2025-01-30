/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use crate::{
    error::{ContractError, GenericResult},
    hex::{decode_hex_arr, hex_from_iter},
};

#[inline]
fn hash_to_field_elem<F: FromUniformBytes<64>>(persona: &[u8], vals: &[&[u8]]) -> F {
    let mut hasher = blake2b_simd::Params::new().hash_length(64).personal(persona).to_state();

    for v in vals {
        hasher.update(v);
    }

    F::from_uniform_bytes(hasher.finalize().as_array())
}

/// Hash a slice of values together with a prefix `persona` using BLAKE2b
/// and return a `pallas::Scalar` element from the digest.
pub fn hash_to_scalar(persona: &[u8], vals: &[&[u8]]) -> pallas::Scalar {
    hash_to_field_elem(persona, vals)
}

/// Hash a slice of values together with a prefix `persona` using BLAKE2b
/// and return a `pallas::Scalar` element from the digest.
pub fn hash_to_base(persona: &[u8], vals: &[&[u8]]) -> pallas::Base {
    hash_to_field_elem(persona, vals)
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

// Not allowed to implement external traits for external crates
pub trait FieldElemAsStr: PrimeField<Repr = [u8; 32]> {
    fn to_string(&self) -> String {
        // We reverse repr since it is little endian encoded
        "0x".to_string() + &hex_from_iter(self.to_repr().iter().cloned().rev())
    }

    fn from_str(hex: &str) -> GenericResult<Self> {
        if hex.len() != 33 * 2 {
            return Err(ContractError::HexFmtErr)
        }

        let hex = hex.strip_prefix("0x").ok_or(ContractError::HexFmtErr)?;

        let mut bytes = decode_hex_arr(hex)?;
        bytes.reverse();

        Option::from(Self::from_repr(bytes)).ok_or(ContractError::HexFmtErr)
    }
}

impl FieldElemAsStr for pallas::Base {}
impl FieldElemAsStr for pallas::Scalar {}

#[test]
fn test_fp_to_u64() {
    use super::pasta_prelude::Field;

    let fp = pallas::Base::from(u64::MAX);
    assert_eq!(fp_to_u64(fp), Some(u64::MAX));
    assert_eq!(fp_to_u64(fp + pallas::Base::ONE), None);
}

#[test]
fn test_fp_to_str() {
    use self::FieldElemAsStr;
    let fpstr = "0x227ae0da79929f3e23f8d5bc9992f5f140f5198932378731e1b49b67fdc296c8";
    assert_eq!(pallas::Base::from_str(fpstr).unwrap().to_string(), fpstr);

    let fpstr = "0x000000000000000000000000000000000000000000000000ffffffffffffffff";
    let fp = pallas::Base::from(u64::MAX);
    assert_eq!(fp.to_string(), fpstr);
    assert_eq!(pallas::Base::from_str(fpstr).unwrap(), fp);
}
