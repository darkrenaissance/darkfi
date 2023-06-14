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

use darkfi_money_contract::model::Coin;
use darkfi_sdk::{
    crypto::{note::AeadEncryptedNote, pasta_prelude::*, MerkleNode, Nullifier, PublicKey},
    error::ContractError,
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

/// A `DaoBulla` represented in the state
#[derive(Debug, Copy, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct DaoBulla(pallas::Base);

impl DaoBulla {
    /// Reference the raw inner base field element
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Create a `DaoBulla` object from given bytes, erroring if the
    /// input bytes are noncanonical.
    pub fn from_bytes(x: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(x).into() {
            Some(v) => Ok(Self(v)),
            None => {
                Err(ContractError::IoError("Failed to instantiate DaoBulla from bytes".to_string()))
            }
        }
    }

    /// Convert the `DaoBulla` type into 32 raw bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }
}

use core::str::FromStr;
darkfi_sdk::fp_from_bs58!(DaoBulla);
darkfi_sdk::fp_to_bs58!(DaoBulla);
darkfi_sdk::ty_from_fp!(DaoBulla);

/// Parameters for `Dao::Mint`
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoMintParams {
    /// The DAO bulla
    pub dao_bulla: DaoBulla,
    /// The DAO public key
    pub dao_pubkey: PublicKey,
}

/// State update for `Dao::Mint`
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoMintUpdate {
    /// Revealed DAO bulla
    pub dao_bulla: DaoBulla,
}

/// Parameters for `Dao::Propose`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoProposeParams {
    /// Merkle root of the DAO in the DAO state
    pub dao_merkle_root: MerkleNode,
    /// Token ID commitment for the proposal
    pub token_commit: pallas::Base,
    /// Bulla of the DAO proposal
    pub proposal_bulla: pallas::Base,
    /// Encrypted note
    pub note: AeadEncryptedNote,
    /// Inputs for the proposal
    pub inputs: Vec<DaoProposeParamsInput>,
}

/// Input for a DAO proposal
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoProposeParamsInput {
    /// Value commitment for the input
    pub value_commit: pallas::Point,
    /// Merkle root for the input's inclusion proof
    pub merkle_root: MerkleNode,
    /// Public key used for signing
    pub signature_public: PublicKey,
}

/// State update for `Dao::Propose`
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoProposeUpdate {
    /// Minted proposal bulla
    pub proposal_bulla: pallas::Base,
    /// Snapshotted Merkle root in the Money state
    pub snapshot_root: MerkleNode,
}

/// Metadata for a DAO proposal on the blockchain
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoProposalMetadata {
    /// Vote aggregate
    pub vote_aggregate: DaoBlindAggregateVote,
    /// Snapshotted Merkle root in the Money state
    pub snapshot_root: MerkleNode,
    /// Proposal closed
    pub ended: bool,
}

/// Parameters for `Dao::Vote`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoVoteParams {
    /// Token commitment for the vote inputs
    pub token_commit: pallas::Base,
    /// Proposal bulla being voted on
    pub proposal_bulla: pallas::Base,
    /// Commitment for yes votes
    pub yes_vote_commit: pallas::Point,
    /// Encrypted note
    pub note: AeadEncryptedNote,
    /// Inputs for the vote
    pub inputs: Vec<DaoVoteParamsInput>,
}

/// Input for a DAO proposal vote
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoVoteParamsInput {
    /// Revealed nullifier
    pub nullifier: Nullifier,
    /// Vote commitment
    pub vote_commit: pallas::Point,
    /// Merkle root for the input's inclusion proof
    pub merkle_root: MerkleNode,
    /// Public key used for signing
    pub signature_public: PublicKey,
}

/// State update for `Dao::Vote`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoVoteUpdate {
    /// The proposal bulla being voted on
    pub proposal_bulla: pallas::Base,
    /// The updated proposal metadata
    pub proposal_metadata: DaoProposalMetadata,
    /// Vote nullifiers,
    pub vote_nullifiers: Vec<Nullifier>,
}

/// Represents a single or multiple blinded votes.
/// These can be summed together.
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoBlindAggregateVote {
    /// Weighted vote commit
    pub yes_vote_commit: pallas::Point,
    /// All value staked in the vote
    pub all_vote_commit: pallas::Point,
}

impl DaoBlindAggregateVote {
    /// Aggregate a vote with existing one
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

/// Parameters for `Dao::Exec`
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoExecParams {
    /// The proposal bulla
    pub proposal: pallas::Base,
    /// The output coin for the proposal recipient
    pub coin_0: Coin,
    /// The output coin for the change returned to DAO
    pub coin_1: Coin,
    /// Aggregated blinds for the vote commitments
    pub blind_total_vote: DaoBlindAggregateVote,
    /// Value commitment for all the inputs
    pub input_value_commit: pallas::Point,
}

/// State update for `Dao::Exec`
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoExecUpdate {
    /// The proposal bulla
    pub proposal: pallas::Base,
}
