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
use log::debug;

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
    // PID controller K values based on constants
    static ref K1: Float10 = KP.clone() + KI.clone() + KD.clone();
    static ref K2: Float10 = FLOAT10_NEG_ONE.clone() * KP.clone() + FLOAT10_NEG_TWO.clone() * KD.clone();
    static ref K3: Float10 = KD.clone();
}

/// Return 2-term target approximation sigma coefficients,
/// alogn with the inverse probability `f` of becoming a
/// block producer and the feedback error, corresponding
/// to provided slot consensus state,
pub fn slot_pid_output(
    previous_slot: &Slot,
    previous_producers: u64,
) -> (f64, f64, pallas::Base, pallas::Base) {
    let (f, error) = calculate_f(previous_slot, previous_producers);
    let total_tokens =
        Float10::try_from(previous_slot.total_tokens + previous_slot.reward).unwrap();
    let (sigma1, sigma2) = calculate_sigmas(f.clone(), total_tokens);

    (f.to_f64(), error.to_f64(), sigma1, sigma2)
}

/// Calculate the inverse probability `f` of becoming a block producer (winning the lottery)
/// having all the tokens, and the feedback error, represented as Float10.
fn calculate_f(previous_slot: &Slot, previous_producers: u64) -> (Float10, Float10) {
    // Convert slot values to Float10
    let previous_slot_f = Float10::try_from(previous_slot.pid.f).unwrap();
    debug!(target: "validator::consensus::pid::calculate_f", "Previous slot f: {previous_slot_f}");
    let previous_slot_error = Float10::try_from(previous_slot.pid.error).unwrap();
    debug!(target: "validator::consensus::pid::calculate_f", "Previous slot error: {previous_slot_error}");
    let previous_slot_previous_slot_error =
        Float10::try_from(previous_slot.previous.error).unwrap();
    debug!(target: "validator::consensus::pid::calculate_f", "Previous slot previous slot error: {previous_slot_previous_slot_error}");

    // Calculate feedback error based on previous block producers.
    let feedback = Float10::try_from(previous_producers).unwrap();
    debug!(target: "validator::consensus::pid::calculate_f", "Feedback: {feedback}");
    let error = FLOAT10_ONE.clone() - feedback;
    debug!(target: "validator::consensus::pid::calculate_f", "Error: {error}");

    // Calculate f
    let mut f = previous_slot_f +
        K1.clone() * error.clone() +
        K2.clone() * previous_slot_error +
        K3.clone() * previous_slot_previous_slot_error;
    debug!(target: "validator::consensus::pid::calculate_f", "Ounbounded f: {f}");

    // Boundaries control
    if f <= *FLOAT10_ZERO {
        f = MIN_F.clone()
    } else if f >= *FLOAT10_ONE {
        f = MAX_F.clone()
    }
    debug!(target: "validator::consensus::pid::calculate_f", "Bounded f: {f}");

    (f, error)
}

/// Return 2-term target approximation sigma coefficients,
/// corresponding to provided `f` and `total_tokens` values.
fn calculate_sigmas(f: Float10, total_tokens: Float10) -> (pallas::Base, pallas::Base) {
    // Calculate `neg_c` value
    let x = FLOAT10_ONE.clone() - f;
    let c = x.ln();
    let neg_c = FLOAT10_NEG_ONE.clone() * c;
    debug!(target: "validator::consensus::pid::calculate_sigmas", "neg_c: {neg_c}");

    // Calculate sigma 1
    let sigma1_fbig = neg_c.clone() / (total_tokens.clone() + EPSILON.clone()) * FIELD_P.clone();
    let sigma1 = fbig2base(sigma1_fbig);
    debug!(target: "validator::consensus::pid::calculate_sigmas", "Sigma 1: {sigma1:?}");

    // Calculate sigma 2
    let sigma2_fbig = (neg_c / (total_tokens + EPSILON.clone())).powf(FLOAT10_TWO.clone()) *
        (FIELD_P.clone() / FLOAT10_TWO.clone());
    let sigma2 = fbig2base(sigma2_fbig);
    debug!(target: "validator::consensus::pid::calculate_sigmas", "Sigma 2: {sigma2:?}");

    (sigma1, sigma2)
}

#[cfg(test)]
mod tests {
    use super::calculate_f;
    use super::Slot;
    // use super::Float10;
    use super::MIN_F;
    use super::MAX_F;

    #[test]
    fn f_is_bounded() {
        // Method: calculate_f takes a slot previous_slot as an argument. 
        // This slot's f value is summed with other low numbers to produce f.
        // By setting the previous_slot's f to a very large value, we can check
        // that calculate_f is properly bounding the result of the sum.
        let mut slot = Slot::default();
        slot.pid.f = -1_000_000.0;
        let (f,_) = calculate_f(&slot, 0);
        assert!(f >= *MIN_F);
        assert!(f <= *MAX_F);

        let mut slot = Slot::default();
        slot.pid.f = 1_000_000.0;
        let (f,_) = calculate_f(&slot, 0);
        assert!(f >= *MIN_F);
        assert!(f <= *MAX_F);
    }
}
