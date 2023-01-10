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

use darkfi_sdk::crypto::{pallas, pasta_prelude::*, MerkleNode, Nullifier, PublicKey};
use darkfi_serial::{SerialDecodable, SerialEncodable};

#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoBulla(pallas::Base);

impl DaoBulla {
    pub fn inner(&self) -> pallas::Base {
        self.0
    }
}

impl From<pallas::Base> for DaoBulla {
    fn from(x: pallas::Base) -> Self {
        Self(x)
    }
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoMintParams {
    pub dao_bulla: DaoBulla,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoMintUpdate {
    pub dao_bulla: DaoBulla,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoProposeParams {
    pub dao_merkle_root: MerkleNode,
    pub token_commit: pallas::Base,
    pub proposal_bulla: pallas::Base,
    pub ciphertext: Vec<u8>,
    pub ephem_public: PublicKey,
    pub inputs: Vec<DaoProposeParamsInput>,
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct DaoProposeParamsInput {
    pub value_commit: pallas::Point,
    pub merkle_root: MerkleNode,
    pub signature_public: PublicKey,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoProposeUpdate {
    pub proposal_bulla: pallas::Base,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoVoteParams {
    pub token_commit: pallas::Base,
    pub proposal_bulla: pallas::Base,
    pub yes_vote_commit: pallas::Point,
    pub ciphertext: Vec<u8>,
    pub ephem_public: PublicKey,
    pub inputs: Vec<DaoVoteParamsInput>,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoVoteParamsInput {
    pub nullifier: Nullifier,
    pub vote_commit: pallas::Point,
    pub merkle_root: MerkleNode,
    pub signature_public: PublicKey,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoVoteUpdate {
    pub proposal_bulla: pallas::Base,
    pub proposal_votes: DaoBlindAggregateVote,
    pub vote_nullifiers: Vec<Nullifier>,
}

/// Represents a single or multiple blinded votes. These can be summed together.
#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoBlindAggregateVote {
    /// Weighted vote commit
    pub yes_vote_commit: pallas::Point,
    /// All value staked in the vote
    pub all_vote_commit: pallas::Point,
}

impl DaoBlindAggregateVote {
    pub fn aggregate(&mut self, other: Self) {
        self.yes_vote_commit += other.yes_vote_commit;
        self.all_vote_commit += other.all_vote_commit;
    }
}

impl Default for DaoBlindAggregateVote {
    fn default() -> Self {
        Self {
            yes_vote_commit: pallas::Point::identity(),
            all_vote_commit: pallas::Point::identity(),
        }
    }
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoExecParams {
    pub proposal: pallas::Base,
    pub coin_0: pallas::Base,
    pub coin_1: pallas::Base,
    pub blind_total_vote: DaoBlindAggregateVote,
    pub input_value_commit: pallas::Point,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoExecUpdate {
    pub proposal: pallas::Base,
}
