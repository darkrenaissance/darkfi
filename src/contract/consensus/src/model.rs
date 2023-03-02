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

use darkfi_money_contract::model::Output;
use darkfi_sdk::{
    crypto::{Coin, MerkleNode, Nullifier},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

/// Anonymous input from `Money::Stake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct StakeInput {
    /// Blinding factor for `token_id`
    pub token_blind: pallas::Scalar,
    /// Pedersen commitment for the staked coin's value
    pub value_commit: pallas::Point,
    /// Revealed nullifier
    pub nullifier: Nullifier,
    /// Revealed Merkle root
    pub merkle_root: MerkleNode,
}

/// Parameters for `Consensus::Stake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusStakeParamsV1 {
    /// Burnt token revealed info
    pub input: StakeInput,
    /// Anonymous output
    pub output: Output,
}

/// State update for `Consensus::Stake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusStakeUpdateV1 {
    /// The newly minted coin
    pub coin: Coin,
}
