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

use darkfi_sdk::{blockchain::Slot, pasta::pallas};
use lazy_static::lazy_static;

use super::float_10::{
    fbig2base, Float10, FLOAT10_NEG_ONE, FLOAT10_NEG_TWO, FLOAT10_ONE, FLOAT10_TWO, FLOAT10_ZERO,
};

/// PID controller configuration
const P: &str = "28948022309329048855892746252171976963363056481941560715954676764349967630337";
lazy_static! {
    static ref FIELD_P: Float10 = Float10::try_from(P).unwrap();
    static ref KP: Float10 = Float10::try_from("0.18").unwrap();
    static ref KI: Float10 = Float10::try_from("0.02").unwrap();
    static ref KD: Float10 = Float10::try_from("-0.1").unwrap();
    static ref MAX_F: Float10 = Float10::try_from("0.99").unwrap();
    static ref MIN_F: Float10 = Float10::try_from("0.01").unwrap();
    static ref EPSILON: Float10 = Float10::try_from("1").unwrap();
}

/// Return 2-term target approximation sigma coefficients,
/// alogn with the inverse probability `f` of becoming a
/// block producer and the feedback error, corresponding
/// to provided slot consensus state,
pub fn slot_pid_output(previous_slot: &Slot) -> (f64, f64, pallas::Base, pallas::Base) {
    let (f, error) = calculate_f(previous_slot);
    let total_tokens =
        Float10::try_from(previous_slot.total_tokens + previous_slot.reward).unwrap();
    let (sigma1, sigma2) = calculate_sigmas(f.clone(), total_tokens);

    // TODO: log values

    (f.to_f64(), error.to_f64(), sigma1, sigma2)
}

/// Calculate the inverse probability `f` of becoming a block producer (winning the lottery)
/// having all the tokens, and the feedback error, represented as Float10.
fn calculate_f(previous_slot: &Slot) -> (Float10, Float10) {
    // PID controller K values based on constants
    let k1 = KP.clone() + KI.clone() + KD.clone();
    let k2 = FLOAT10_NEG_ONE.clone() * KP.clone() + FLOAT10_NEG_TWO.clone() * KD.clone();
    let k3 = KD.clone();

    // Convert slot values to Float10
    let previous_slot_f = Float10::try_from(previous_slot.f).unwrap();
    let previous_slot_error = Float10::try_from(previous_slot.error).unwrap();
    let previous_slot_previous_slot_error =
        Float10::try_from(previous_slot.previous_slot_error).unwrap();

    // Calculate feedback error based on previous block producers.
    // We know how many producers existed in previous slot by
    // the len of its fork hashes.
    let feedback = Float10::try_from(previous_slot.fork_hashes.len() as u64).unwrap();
    let error = FLOAT10_ONE.clone() - feedback;

    // Calculate f
    let mut f = previous_slot_f +
        k1 * error.clone() +
        k2 * previous_slot_error +
        k3 * previous_slot_previous_slot_error;

    // Boundaries control
    if f <= *FLOAT10_ZERO {
        f = MIN_F.clone()
    } else if f >= *FLOAT10_ONE {
        f = MAX_F.clone()
    }

    (f, error)
}

/// Return 2-term target approximation sigma coefficients,
/// corresponding to provided `f` and `total_tokens` values.
fn calculate_sigmas(f: Float10, total_tokens: Float10) -> (pallas::Base, pallas::Base) {
    // Calculate `neg_c` value
    let x = FLOAT10_ONE.clone() - f;
    let c = x.ln();
    let neg_c = FLOAT10_NEG_ONE.clone() * c;

    // Calculate sigma 1
    let sigma1_fbig = neg_c.clone() / (total_tokens.clone() + EPSILON.clone()) * FIELD_P.clone();
    let sigma1 = fbig2base(sigma1_fbig);

    // Calculate sigma 2
    let sigma2_fbig = (neg_c / (total_tokens + EPSILON.clone())).powf(FLOAT10_TWO.clone()) *
        (FIELD_P.clone() / FLOAT10_TWO.clone());
    let sigma2 = fbig2base(sigma2_fbig);

    (sigma1, sigma2)
}
