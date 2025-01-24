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

//! <https://vitalik.eth.limo/general/2018/07/21/starks_part_3.html>

use num_bigint::BigUint;
use num_traits::Num;

/// Modulus of prime field 2^256 - 2^32 * 351 + 1
const MODULUS: &str =
    "115792089237316195423570985008687907853269984665640564039457584006405596119041";

/// An exponent to perform inverse of x^3 on prime field based on Fermat's Little Theorem
const L_FERMAT_EXPONENT: &str =
    "77194726158210796949047323339125271902179989777093709359638389337603730746027";

/// Calculates set of round constants to perform MiMC-calculation on.
fn calculate_round_constants() -> [BigUint; 64] {
    let mut round_constants: Vec<BigUint> = vec![];
    #[allow(clippy::needless_range_loop)]
    for i in 0u64..64 {
        round_constants.push(BigUint::from(i).pow(7) ^ BigUint::from(42u64));
    }

    round_constants.try_into().unwrap()
}

/// Executes `num_steps` of MiMC-calculation in forward direction for the given `input`
fn forward_mimc(num_steps: u64, input: &BigUint) -> BigUint {
    let modulus = BigUint::from_str_radix(MODULUS, 10).unwrap();
    let round_constants = calculate_round_constants();

    let mut result = input.clone();
    let three = BigUint::from(3_u64);
    for i in 1..num_steps {
        result = (result.modpow(&three, &modulus) +
            &round_constants[i as usize % round_constants.len()])
            .modpow(&BigUint::from(1_u64), &modulus);
    }

    result
}

/// Executes `num_steps` of MiMC-calculation in backward direction for the given `input`.
///
/// The properties of MiMC-scheme guarantees that calculation in backward direction is
/// always slower than in forward for correctly chosen parameters.
fn backward_mimc(num_steps: u64, input: &BigUint) -> BigUint {
    let modulus = BigUint::from_str_radix(MODULUS, 10).unwrap();
    let l_fermat_exp = BigUint::from_str_radix(L_FERMAT_EXPONENT, 10).unwrap();
    let round_constants = calculate_round_constants();

    let mut result = input.clone();
    for i in (1..num_steps).rev() {
        let round_constant = &round_constants[i as usize % round_constants.len()];
        result = (&result - round_constant).modpow(&l_fermat_exp, &modulus);
    }

    result
}

/// Performs an Eval() step of the MiMC-based VDF
pub fn eval(seed: &BigUint, num_steps: u64) -> BigUint {
    backward_mimc(num_steps, seed)
}

/// Performs a Verify() step for the MiMC-based VDF result
pub fn verify(seed: &BigUint, num_steps: u64, witness: &BigUint) -> bool {
    forward_mimc(num_steps, witness) == *seed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mimc_vdf_eval_and_verify() {
        let steps = 1000;
        let challenge = blake3::hash(b"69420").to_hex();
        let challenge = BigUint::from_str_radix(&challenge, 16).unwrap();

        let witness = eval(&challenge, steps);
        assert!(verify(&challenge, steps, &witness));
        assert!(!verify(&(&challenge - 1_u64), steps, &witness));
        assert!(!verify(&challenge, steps - 1, &witness));
        assert!(!verify(&challenge, steps, &(&witness - 1_u64)));
    }
}
