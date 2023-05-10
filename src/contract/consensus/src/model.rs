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

use darkfi_money_contract::model::{Input, Output, StakeInput};
use darkfi_sdk::pasta::pallas;
use darkfi_serial::{SerialDecodable, SerialEncodable};

/// Parameters for `Consensus::ProposalReward`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusProposalRewardParamsV1 {
    /// Anonymous input of `Consensus::Unstake`
    pub unstake_input: Input,
    /// Burnt token revealed info of `Consensus::Stake`
    pub stake_input: StakeInput,
    /// Anonymous output
    pub output: Output,
    /// Rewarded slot
    pub slot: u64,
    /// Coin y
    pub y: pallas::Base,
    /// Lottery rho used
    pub rho: pallas::Base,
}

/// Parameters for `Consensus::ProposalMint`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusProposalMintParamsV1 {
    /// Burnt token revealed info
    pub input: StakeInput,
    /// Anonymous output
    pub output: Output,
    /// Pedersen commitment for the output's serial number
    pub serial_commit: pallas::Point,
}

/// State update for `Consensus::Reward`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusProposalRewardUpdateV1 {}

// Consensus parameters configuration.
// Note: Always verify `pallas::Base` are correct, in case of changes,
// using pallas_constants tool.
// Configured reward
pub const REWARD: u64 = 1;
// Reward `pallas::Base`, calculated by: pallas::Base::from(REWARD)
pub const REWARD_PALLAS: pallas::Base = pallas::Base::from_raw([1, 0, 0, 0]);
// `pallas::Base` used as prefix/suffix in poseidon hash
pub const ZERO: pallas::Base = pallas::Base::zero();
// Serial prefix, calculated by: pallas::Base::from(2)
pub const SERIAL_PREFIX: pallas::Base = pallas::Base::from_raw([2, 0, 0, 0]);
// Seed prefix, calculated by: pallas::Base::from(3)
pub const SEED_PREFIX: pallas::Base = pallas::Base::from_raw([3, 0, 0, 0]);
// Election seed y prefix, calculated by: pallas::Base::from(22)
pub const MU_Y_PREFIX: pallas::Base = pallas::Base::from_raw([22, 0, 0, 0]);
// Election seed rho prefix, calculated by: pallas::Base::from(5)
pub const MU_RHO_PREFIX: pallas::Base = pallas::Base::from_raw([5, 0, 0, 0]);
// Lottery headstart, calculated by: darkfi::consensus::LeadCoin::headstart()
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
