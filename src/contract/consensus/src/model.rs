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

/// Parameters for `Consensus::Reward`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusRewardParamsV1 {
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

/// State update for `Consensus::Reward`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusRewardUpdateV1 {}

// TODO: Don't set these here
pub const REWARD: u64 = 1;

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
