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
        sigma1: pallas::Base,
        sigma2: pallas::Base,
    ) -> Self {
        Self { id, previous_eta, fork_hashes, fork_previous_hashes, sigma1, sigma2 }
    }

    /// Generate the genesis slot.
    pub fn genesis_slot(genesis_block: blake3::Hash) -> Self {
        let previous_eta = pallas::Base::ZERO;
        let fork_hashes = vec![];
        // Since genesis block has no previous,
        // we will use its own hash as its previous.
        let fork_previous_hashes = vec![genesis_block];
        let sigma1 = pallas::Base::ZERO;
        let sigma2 = pallas::Base::ZERO;

        Self::new(0, previous_eta, fork_hashes, fork_previous_hashes, sigma1, sigma2)
    }
}
