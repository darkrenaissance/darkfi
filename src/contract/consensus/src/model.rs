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
// ANCHOR: ConsensusProposalParams
pub struct ConsensusProposalParamsV1 {
    /// Anonymous input
    pub input: ConsensusInput,
    /// Anonymous output
    pub output: ConsensusOutput,
    /// Reward value
    pub reward: u64,
    /// Revealed blinding factor for reward value
    pub reward_blind: pallas::Scalar,
    /// Rewarded slot
    pub slot: u64,
    /// Extending fork last proposal/block hash
    pub fork_hash: blake3::Hash,
    /// Extending fork second to last proposal/block hash
    pub fork_previous_hash: blake3::Hash,
    /// VRF proof for eta calculation
    pub vrf_proof: VrfProof,
    /// Coin y
    pub y: pallas::Base,
    /// Lottery rho used
    pub rho: pallas::Base,
}
// ANCHOR_END: ConsensusProposalParams

/// State update for `Consensus::Proposal`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
// ANCHOR: ConsensusProposalUpdate
pub struct ConsensusProposalUpdateV1 {
    /// Revealed nullifier
    pub nullifier: Nullifier,
    /// The newly minted coin
    pub coin: Coin,
}
// ANCHOR_END: ConsensusProposalUpdate

/// Parameters for `Consensus::UnstakeRequest`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusUnstakeRequestParamsV1 {
    /// Burnt token revealed info
    pub input: ConsensusInput,
    /// Anonymous output
    pub output: Output,
}

// ======================================================================
// Consensus parameters configuration
// NOTE: In case of changes, always verify that the `pallas::Base` consts
// are correct using the `pallas_constants` tool in `script/research/`.
// ======================================================================
/// Number of slots in one epoch
pub const EPOCH_LENGTH: u64 = 10;
/// Slot time in seconds
pub const SLOT_TIME: u64 = 90;
// Stake/Unstake timelock length in epochs
pub const GRACE_PERIOD: u64 = calculate_grace_period();
/// Configured block reward (1 DRK == 1 * 10^8)
pub const REWARD: u64 = 100_000_000;
/// Reward `pallas::Base`, calculated by: pallas::Base::from(REWARD)
pub const REWARD_PALLAS: pallas::Base = pallas::Base::from_raw([100000000, 0, 0, 0]);
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

/// Auxiliary function to calculate the grace (locked) period, denominated
/// in epochs.
#[inline]
pub const fn calculate_grace_period() -> u64 {
    // Grace period days target
    const GRACE_PERIOD_DAYS: u64 = 2;

    // 86400 seconds in a day
    (86400 * GRACE_PERIOD_DAYS) / (SLOT_TIME * EPOCH_LENGTH)
}
