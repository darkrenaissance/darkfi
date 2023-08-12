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

use darkfi_serial::{SerialDecodable, SerialEncodable};
use pasta_curves::{group::ff::Field, pallas};

/// Auxiliary structure used to keep track of slots' previous slot
/// relevant validation parameters.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct PreviousSlot {
    /// Block producers count
    pub producers: u64,
    /// Existing forks last proposal/block hashes,
    /// as observed by the validator
    pub last_hashes: Vec<blake3::Hash>,
    /// Existing forks second to last proposal/block hashes,
    /// as observed by the validator
    pub second_to_last_hashes: Vec<blake3::Hash>,
    /// Feedback error
    pub error: f64,
}

impl PreviousSlot {
    pub fn new(
        producers: u64,
        last_hashes: Vec<blake3::Hash>,
        second_to_last_hashes: Vec<blake3::Hash>,
        error: f64,
    ) -> Self {
        Self { producers, last_hashes, second_to_last_hashes, error }
    }
}

impl Default for PreviousSlot {
    /// Represents the genesis slot previous slot on current timestamp
    fn default() -> Self {
        Self::new(0, vec![], vec![], 0.0)
    }
}

/// Auxiliary structure used to keep track of slot PID output.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct PidOutput {
    /// Inverse probability `f` of becoming a block producer
    pub f: f64,
    /// Feedback error
    pub error: f64,
    /// Slot sigma1
    pub sigma1: pallas::Base,
    /// Slot sigma2
    pub sigma2: pallas::Base,
}

impl PidOutput {
    pub fn new(f: f64, error: f64, sigma1: pallas::Base, sigma2: pallas::Base) -> Self {
        Self { f, error, sigma1, sigma2 }
    }
}

impl Default for PidOutput {
    /// Represents the genesis slot PID output on current timestamp
    fn default() -> Self {
        Self::new(0.0, 0.0, pallas::Base::ZERO, pallas::Base::ZERO)
    }
}

/// Auxiliary structure used to keep track of slot validation parameters.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Slot {
    /// Slot UID
    pub id: u64,
    /// Previous slot information
    pub previous: PreviousSlot,
    /// Slot PID output
    pub pid: PidOutput,
    /// Last block/proposal eta
    pub last_eta: pallas::Base,
    /// Total tokens up until this slot
    pub total_tokens: u64,
    /// Slot reward
    pub reward: u64,
}

impl Slot {
    pub fn new(
        id: u64,
        previous: PreviousSlot,
        pid: PidOutput,
        last_eta: pallas::Base,
        total_tokens: u64,
        reward: u64,
    ) -> Self {
        Self { id, previous, pid, last_eta, total_tokens, reward }
    }
}

impl Default for Slot {
    /// Represents the genesis slot on current timestamp
    fn default() -> Self {
        Self::new(0, PreviousSlot::default(), PidOutput::default(), pallas::Base::ZERO, 0, 0)
    }
}
