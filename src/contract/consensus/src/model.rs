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

use darkfi_money_contract::model::{ClearInput, Coin, ConsensusInput, ConsensusOutput, Output};
use darkfi_sdk::{
    crypto::{ecvrf::VrfProof, Nullifier},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

/// Parameters for `Consensus::GenesisStake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
// ANCHOR: ConsensusGenesisStakeParams
pub struct ConsensusGenesisStakeParamsV1 {
    /// Clear input
    pub input: ClearInput,
    /// Anonymous output
    pub output: ConsensusOutput,
}
// ANCHOR_END: ConsensusGenesisStakeParams

/// Parameters for `Consensus::Proposal`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusProposalParamsV1 {
    /// Anonymous input
    pub input: ConsensusInput,
    /// Anonymous output
    pub output: ConsensusOutput,
    /// Reward value
    pub reward: u64,
    /// Blinding factor for reward value
    pub reward_blind: pallas::Scalar,
    /// Pedersen commitment for the output's serial number
    pub new_serial_commit: pallas::Point,
    /// Rewarded slot
    pub slot: u64,
    /// VRF proof for eta calculation
    pub vrf_proof: VrfProof,
    /// Coin y
    pub y: pallas::Base,
    /// Lottery rho used
    pub rho: pallas::Base,
}

/// State update for `Consensus::Proposal`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusProposalUpdateV1 {
    /// Revealed nullifier
    pub nullifier: Nullifier,
    /// The newly minted coin
    pub coin: Coin,
}

/// Parameters for `Consensus::UnstakeRequest`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusUnstakeRequestParamsV1 {
    /// Burnt token revealed info
    pub input: ConsensusInput,
    /// Anonymous output
    pub output: Output,
}

// ==================================
// Consensus parameters configuration
// Note: Always verify `pallas::Base` are correct, in case of changes,
// using pallas_constants tool.
// ==================================

/// Number of slots in one epoch
pub const EPOCH_LENGTH: u64 = 10;
/// Slot time in seconds
pub const SLOT_TIME: u64 = 90;
/// Grace period days target
pub const GRACE_PERIOD_DAYS: u64 = 2;
/// Configured block reward
pub const REWARD: u64 = 1;
/// Reward `pallas::Base`, calculated by: pallas::Base::from(REWARD)
pub const REWARD_PALLAS: pallas::Base = pallas::Base::from_raw([1, 0, 0, 0]);
/// Serial prefix, calculated by: pallas::Base::from(2)
pub const SERIAL_PREFIX: pallas::Base = pallas::Base::from_raw([2, 0, 0, 0]);
/// Seed prefix, calculated by: pallas::Base::from(3)
pub const SEED_PREFIX: pallas::Base = pallas::Base::from_raw([3, 0, 0, 0]);
/// Election seed y prefix, calculated by: pallas::Base::from(22)
pub const MU_Y_PREFIX: pallas::Base = pallas::Base::from_raw([22, 0, 0, 0]);
/// Election seed rho prefix, calculated by: pallas::Base::from(5)
pub const MU_RHO_PREFIX: pallas::Base = pallas::Base::from_raw([5, 0, 0, 0]);
/// Lottery headstart, calculated by: darkfi::consensus::LeadCoin::headstart()
pub const HEADSTART: pallas::Base = pallas::Base::from_raw([
    11731824086999220879,
    11830614503713258191,
    737869762948382064,
    46116860184273879,
]);

/// Auxiliary structure to decode `darkfi::consensus::state::SlotCheckpoint`
/// to use in contract.
#[derive(SerialDecodable)]
pub struct SlotCheckpoint {
    /// Slot UID
    pub slot: u64,
    /// Slot eta
    pub eta: pallas::Base,
    /// Slot sigma1
    pub sigma1: pallas::Base,
    /// Slot sigma2
    pub sigma2: pallas::Base,
}

/// Auxiliary function to calculate the grace(locked) period, denominated
/// in epochs.
#[inline]
pub const fn calculate_grace_period() -> u64 {
    // 86400 seconds in a day
    (86400 * GRACE_PERIOD_DAYS) / (SLOT_TIME * EPOCH_LENGTH)
}
