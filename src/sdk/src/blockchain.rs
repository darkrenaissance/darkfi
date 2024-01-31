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

#[cfg(feature = "async")]
use darkfi_serial::async_trait;
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
    /// Last block/proposal nonce(eta)
    pub last_nonce: pallas::Base,
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
        last_nonce: pallas::Base,
        total_tokens: u64,
        reward: u64,
    ) -> Self {
        Self { id, previous, pid, last_nonce, total_tokens, reward }
    }
}

impl Default for Slot {
    /// Represents the genesis slot on current timestamp
    fn default() -> Self {
        Self::new(0, PreviousSlot::default(), PidOutput::default(), pallas::Base::ZERO, 0, 0)
    }
}

// TODO: This values are experimental, should be replaced with the proper ones once defined
pub const POW_CUTOFF: u64 = 1000000;
pub const POS_START: u64 = 1000001;

/// Auxiliary function to calculate provided block height(slot) block version.
/// PoW blocks use version 1, while PoS ones use version 2.
pub fn block_version(height: u64) -> u8 {
    match height {
        0..=POW_CUTOFF => 1,
        POS_START.. => 2,
    }
}

/// Auxiliary function to calculate provided block height epoch.
/// Each epoch is defined by the fixed intervals rewards change.
/// Genesis block is on epoch 0.
pub fn block_epoch(height: u64) -> u64 {
    match height {
        0 => 0,
        1..=1000 => 1,
        1001..=2000 => 2,
        2001..=3000 => 3,
        3001..=4000 => 4,
        4001..=5000 => 5,
        5001..=6000 => 6,
        6001..=7000 => 7,
        7001..=8000 => 8,
        8001..=9000 => 9,
        9001..=10000 => 10,
        10001.. => 11,
    }
}

/// Auxiliary function to calculate provided block height expected reward value.
/// Genesis block always returns reward value 0. Rewards are halfed at fixed intervals,
/// called epochs. After last epoch has started, reward value is based on DARK token-economics.
pub fn expected_reward(height: u64) -> u64 {
    // Grab block height epoch
    let epoch = block_epoch(height);

    // TODO (res) implement reward mechanism with accord to DRK, DARK token-economics.
    // Configured block rewards (1 DRK == 1 * 10^8)
    match epoch {
        0 => 0,
        1 => 2_000_000_000, // 20 DRK
        2 => 1_800_000_000, // 18 DRK
        3 => 1_600_000_000, // 16 DRK
        4 => 1_400_000_000, // 14 DRK
        5 => 1_200_000_000, // 12 DRK
        6 => 1_000_000_000, // 10 DRK
        7 => 800_000_000,   // 8 DRK
        8 => 600_000_000,   // 6 DRK
        9 => 400_000_000,   // 4 DRK
        10 => 200_000_000,  // 2 DRK
        _ => 100_000_000,   // 1 DRK
    }
}
