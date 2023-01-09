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

// DAO::mint()

#[derive(SerialEncodable, SerialDecodable)]
pub struct MintCallParams {
    pub dao_bulla: DaoBulla,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct MintCallUpdate {
    pub dao_bulla: DaoBulla,
}

// DAO::propose()

#[derive(SerialEncodable, SerialDecodable)]
pub struct ProposeCallParams {
    pub dao_merkle_root: MerkleNode,
    pub token_commit: pallas::Base,
    pub proposal_bulla: pallas::Base,
    pub ciphertext: Vec<u8>,
    pub ephem_public: PublicKey,
    pub inputs: Vec<ProposeCallParamsInput>,
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct ProposeCallParamsInput {
    pub value_commit: pallas::Point,
    pub merkle_root: MerkleNode,
    pub signature_public: PublicKey,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct ProposeCallUpdate {
    pub proposal_bulla: pallas::Base,
}

// DAO::vote()

#[derive(SerialEncodable, SerialDecodable)]
pub struct VoteCallParams {
    pub token_commit: pallas::Base,
    pub proposal_bulla: pallas::Base,
    pub yes_vote_commit: pallas::Point,
    pub ciphertext: Vec<u8>,
    pub ephem_public: PublicKey,
    pub inputs: Vec<VoteCallParamsInput>,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct VoteCallParamsInput {
    pub nullifier: Nullifier,
    pub vote_commit: pallas::Point,
    pub merkle_root: MerkleNode,
    pub signature_public: PublicKey,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct VoteCallUpdate {
    pub proposal_bulla: pallas::Base,
    pub proposal_votes: BlindAggregateVote,
    pub vote_nullifiers: Vec<Nullifier>,
}

/// Represents a single or multiple blinded votes. These can be summed together.
#[derive(SerialEncodable, SerialDecodable)]
pub struct BlindAggregateVote {
    /// Weighted vote commit
    pub yes_votes_commit: pallas::Point,
    /// All value staked in the vote
    pub all_votes_commit: pallas::Point,
}

impl BlindAggregateVote {
    //pub fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
    //    self.vote_nullifiers.iter().any(|n| n == nullifier)
    //}

    pub fn combine(&mut self, other: BlindAggregateVote) {
        self.yes_votes_commit += other.yes_votes_commit;
        self.all_votes_commit += other.all_votes_commit;
    }
}

impl Default for BlindAggregateVote {
    fn default() -> Self {
        Self {
            yes_votes_commit: pallas::Point::identity(),
            all_votes_commit: pallas::Point::identity(),
        }
    }
}

// DAO::exec()

#[derive(SerialEncodable, SerialDecodable)]
pub struct ExecCallParams {
    pub proposal: pallas::Base,
    pub coin_0: pallas::Base,
    pub coin_1: pallas::Base,
    pub yes_votes_commit: pallas::Point,
    pub all_votes_commit: pallas::Point,
    pub input_value_commit: pallas::Point,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct ExecCallUpdate {
    pub proposal: pallas::Base,
}
