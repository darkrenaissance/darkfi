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

use halo2_gadgets::ecc::chip::FixedPoint;
use pasta_curves::{arithmetic::CurveExt, pallas};

use super::{
    blind::ScalarBlind,
    constants::{
        fixed_bases::{
            VALUE_COMMITMENT_PERSONALIZATION, VALUE_COMMITMENT_R_BYTES, VALUE_COMMITMENT_V_BYTES,
        },
        NullifierK,
    },
    util::fp_mod_fv,
};

/// Pedersen commitment for a full-width base field element.
#[allow(non_snake_case)]
pub fn pedersen_commitment_base(value: pallas::Base, blind: ScalarBlind) -> pallas::Point {
    let hasher = pallas::Point::hash_to_curve(VALUE_COMMITMENT_PERSONALIZATION);
    let V = NullifierK.generator();
    let R = hasher(&VALUE_COMMITMENT_R_BYTES);

    V * fp_mod_fv(value) + R * blind.inner()
}

/// Pedersen commitment for a 64-bit value, in the base field.
#[allow(non_snake_case)]
pub fn pedersen_commitment_u64(value: u64, blind: ScalarBlind) -> pallas::Point {
    let hasher = pallas::Point::hash_to_curve(VALUE_COMMITMENT_PERSONALIZATION);
    let V = hasher(&VALUE_COMMITMENT_V_BYTES);
    let R = hasher(&VALUE_COMMITMENT_R_BYTES);

    V * fp_mod_fv(pallas::Base::from(value)) + R * blind.inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pedersen_commitment() {
        let a_value = pallas::Base::from(10);
        let a_blind = ScalarBlind::from(11);
        let b_value = pallas::Base::from(20);
        let b_blind = ScalarBlind::from(21);

        assert_eq!(
            pedersen_commitment_base(a_value, a_blind) + pedersen_commitment_base(b_value, b_blind),
            pedersen_commitment_base(a_value + b_value, &a_blind + &b_blind)
        );

        let a_value = 10;
        let b_value = 20;

        assert_eq!(
            pedersen_commitment_u64(a_value, a_blind) + pedersen_commitment_u64(b_value, b_blind),
            pedersen_commitment_u64(a_value + b_value, &a_blind + &b_blind)
        );
    }
}
