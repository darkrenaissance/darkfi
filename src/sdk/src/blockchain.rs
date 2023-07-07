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

/// Auxiliary structure used to keep track of slot validation parameters.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Slot {
    /// Slot UID
    pub id: u64,
    /// Previous slot eta
    pub previous_eta: pallas::Base,
    /// Previous slot forks last proposal/block hashes,
    /// as observed by the validator
    pub fork_hashes: Vec<blake3::Hash>,
    /// Previous slot second to last proposal/block hashes,
    /// as observed by the validator
    pub fork_previous_hashes: Vec<blake3::Hash>,
    /// Slot inverse probability `f` of becoming a block producer
    pub f: f64,
    /// Slot feedback error
    pub error: f64,
    /// Previous slot feedback error
    pub previous_slot_error: f64,
    /// Total tokens up until this slot
    pub total_tokens: u64,
    /// Slot reward
    pub reward: u64,
    /// Slot sigma1
    pub sigma1: pallas::Base,
    /// Slot sigma2
    pub sigma2: pallas::Base,
}

impl Slot {
    pub fn new(
        id: u64,
        previous_eta: pallas::Base,
        fork_hashes: Vec<blake3::Hash>,
        fork_previous_hashes: Vec<blake3::Hash>,
        f: f64,
        error: f64,
        previous_slot_error: f64,
        total_tokens: u64,
        reward: u64,
        sigma1: pallas::Base,
        sigma2: pallas::Base,
    ) -> Self {
        Self {
            id,
            previous_eta,
            fork_hashes,
            fork_previous_hashes,
            f,
            error,
            previous_slot_error,
            total_tokens,
            reward,
            sigma1,
            sigma2,
        }
    }
}

impl Default for Slot {
    /// Represents the genesis slot on current timestamp
    fn default() -> Self {
        Self::new(
            0,
            pallas::Base::ZERO,
            vec![],
            vec![],
            0.0,
            0.0,
            0.0,
            0,
            0,
            pallas::Base::ZERO,
            pallas::Base::ZERO,
        )
    }
}
