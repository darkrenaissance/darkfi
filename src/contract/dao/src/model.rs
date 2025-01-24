/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use core::str::FromStr;

use darkfi_money_contract::model::{Nullifier, TokenId};
use darkfi_sdk::{
    crypto::{
        note::{AeadEncryptedNote, ElGamalEncryptedNote},
        pasta_prelude::*,
        poseidon_hash, BaseBlind, ContractId, MerkleNode, PublicKey,
    },
    error::ContractError,
    pasta::pallas,
};
use darkfi_serial::{Encodable, SerialDecodable, SerialEncodable};

#[cfg(feature = "client")]
use darkfi_serial::async_trait;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
// ANCHOR: dao
/// DAOs are represented on chain as a commitment to this object
pub struct Dao {
    /// The minimum amount of governance tokens needed to open a proposal
    pub proposer_limit: u64,
    /// Minimal threshold of participating total tokens needed for a proposal to pass
    pub quorum: u64,
    /// Minimal threshold of participating total tokens needed for a proposal to
    /// be considered as strongly supported, enabling early execution.
    /// Must be greater or equal to normal quorum.
    pub early_exec_quorum: u64,
    /// The ratio of winning/total votes needed for a proposal to pass
    pub approval_ratio_quot: u64,
    pub approval_ratio_base: u64,
    /// DAO's governance token ID
    pub gov_token_id: TokenId,
    /// DAO notes decryption public key
    pub notes_public_key: PublicKey,
    /// DAO proposals creator public key
    pub proposer_public_key: PublicKey,
    /// DAO proposals viewer public key
    pub proposals_public_key: PublicKey,
    /// DAO votes viewer public key
    pub votes_public_key: PublicKey,
    /// DAO proposals executor public key
    pub exec_public_key: PublicKey,
    /// DAO strongly supported proposals executor public key
    pub early_exec_public_key: PublicKey,
    /// DAO bulla blind
    pub bulla_blind: BaseBlind,
}
// ANCHOR_END: dao

impl Dao {
    pub fn to_bulla(&self) -> DaoBulla {
        let proposer_limit = pallas::Base::from(self.proposer_limit);
        let quorum = pallas::Base::from(self.quorum);
        let early_exec_quorum = pallas::Base::from(self.early_exec_quorum);
        let approval_ratio_quot = pallas::Base::from(self.approval_ratio_quot);
        let approval_ratio_base = pallas::Base::from(self.approval_ratio_base);
        let (notes_pub_x, notes_pub_y) = self.notes_public_key.xy();
        let (proposer_pub_x, proposer_pub_y) = self.proposer_public_key.xy();
        let (proposals_pub_x, proposals_pub_y) = self.proposals_public_key.xy();
        let (votes_pub_x, votes_pub_y) = self.votes_public_key.xy();
        let (exec_pub_x, exec_pub_y) = self.exec_public_key.xy();
        let (early_exec_pub_x, early_exec_pub_y) = self.early_exec_public_key.xy();
        let bulla = poseidon_hash([
            proposer_limit,
            quorum,
            early_exec_quorum,
            approval_ratio_quot,
            approval_ratio_base,
            self.gov_token_id.inner(),
            notes_pub_x,
            notes_pub_y,
            proposer_pub_x,
            proposer_pub_y,
            proposals_pub_x,
            proposals_pub_y,
            votes_pub_x,
            votes_pub_y,
            exec_pub_x,
            exec_pub_y,
            early_exec_pub_x,
            early_exec_pub_y,
            self.bulla_blind.inner(),
        ]);
        DaoBulla(bulla)
    }
}

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

impl std::hash::Hash for DaoBulla {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write(&self.to_bytes());
    }
}

darkfi_sdk::fp_from_bs58!(DaoBulla);
darkfi_sdk::fp_to_bs58!(DaoBulla);
darkfi_sdk::ty_from_fp!(DaoBulla);

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
// ANCHOR: dao-auth-call
pub struct DaoAuthCall {
    pub contract_id: ContractId,
    pub function_code: u8,
    pub auth_data: Vec<u8>,
}
// ANCHOR_END: dao-auth-call

pub trait VecAuthCallCommit {
    fn commit(&self) -> pallas::Base;
}

impl VecAuthCallCommit for Vec<DaoAuthCall> {
    fn commit(&self) -> pallas::Base {
        // Hash a bunch of data, then convert it so pallas::Base
        // see https://docs.rs/ff/0.13.0/ff/trait.FromUniformBytes.html
        // We essentially create a really large value and reduce it modulo the field
        // to diminish the statistical significance of any overlap.
        //
        // The range of pallas::Base is [0, p-1] where p < u256 (=32 bytes).
        // For those values produced by blake3 hash which are [p, u256::MAX],
        // they get mapped to [0, u256::MAX - p].
        // Those 32 bits of pallas::Base are hashed to more frequently.
        // note: blake2 is more secure but slower than blake3
        let mut hasher =
            blake2b_simd::Params::new().hash_length(64).personal(b"justDAOthings").to_state();
        self.encode(&mut hasher).unwrap();
        let hash = hasher.finalize();
        let bytes = hash.as_array();
        pallas::Base::from_uniform_bytes(bytes)
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
// ANCHOR: dao-proposal
pub struct DaoProposal {
    pub auth_calls: Vec<DaoAuthCall>,
    pub creation_blockwindow: u64,
    pub duration_blockwindows: u64,
    /// Arbitrary data provided by the user. We don't use this.
    pub user_data: pallas::Base,
    pub dao_bulla: DaoBulla,
    pub blind: BaseBlind,
}
// ANCHOR_END: dao-proposal

impl DaoProposal {
    pub fn to_bulla(&self) -> DaoProposalBulla {
        let bulla = poseidon_hash([
            self.auth_calls.commit(),
            pallas::Base::from(self.creation_blockwindow),
            pallas::Base::from(self.duration_blockwindows),
            self.user_data,
            self.dao_bulla.inner(),
            self.blind.inner(),
        ]);
        DaoProposalBulla(bulla)
    }
}

/// A `DaoProposalBulla` represented in the state
#[derive(Debug, Copy, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct DaoProposalBulla(pallas::Base);

impl DaoProposalBulla {
    /// Reference the raw inner base field element
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Create a `DaoBulla` object from given bytes, erroring if the
    /// input bytes are noncanonical.
    pub fn from_bytes(x: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(x).into() {
            Some(v) => Ok(Self(v)),
            None => Err(ContractError::IoError(
                "Failed to instantiate DaoProposalBulla from bytes".to_string(),
            )),
        }
    }

    /// Convert the `DaoBulla` type into 32 raw bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }
}

impl std::hash::Hash for DaoProposalBulla {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write(&self.to_bytes());
    }
}

darkfi_sdk::fp_from_bs58!(DaoProposalBulla);
darkfi_sdk::fp_to_bs58!(DaoProposalBulla);
darkfi_sdk::ty_from_fp!(DaoProposalBulla);

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
// ANCHOR: dao-mint-params
/// Parameters for `Dao::Mint`
pub struct DaoMintParams {
    /// The DAO bulla
    pub dao_bulla: DaoBulla,
    /// The DAO signature(notes) public key
    pub dao_pubkey: PublicKey,
}
// ANCHOR_END: dao-mint-params

/// State update for `Dao::Mint`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoMintUpdate {
    /// Revealed DAO bulla
    pub dao_bulla: DaoBulla,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
// ANCHOR: dao-propose-params
/// Parameters for `Dao::Propose`
pub struct DaoProposeParams {
    /// Merkle root of the DAO in the DAO state
    pub dao_merkle_root: MerkleNode,
    /// Token ID commitment for the proposal
    pub token_commit: pallas::Base,
    /// Bulla of the DAO proposal
    pub proposal_bulla: DaoProposalBulla,
    /// Encrypted note
    pub note: AeadEncryptedNote,
    /// Inputs for the proposal
    pub inputs: Vec<DaoProposeParamsInput>,
}
// ANCHOR_END: dao-propose-params

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
// ANCHOR: dao-propose-params-input
/// Input for a DAO proposal
pub struct DaoProposeParamsInput {
    /// Value commitment for the input
    pub value_commit: pallas::Point,
    /// Merkle root for the input's coin inclusion proof
    pub merkle_coin_root: MerkleNode,
    /// SMT root for the input's nullifier exclusion proof
    pub smt_null_root: pallas::Base,
    /// Public key used for signing
    pub signature_public: PublicKey,
}
// ANCHOR_END: dao-propose-params-input

/// State update for `Dao::Propose`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoProposeUpdate {
    /// Minted proposal bulla
    pub proposal_bulla: DaoProposalBulla,
    /// Snapshotted Merkle root in the Money state
    pub snapshot_coins: MerkleNode,
    /// Snapshotted SMT root in the Money state
    pub snapshot_nulls: pallas::Base,
}

/// Metadata for a DAO proposal on the blockchain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoProposalMetadata {
    /// Vote aggregate
    pub vote_aggregate: DaoBlindAggregateVote,
    /// Snapshotted Merkle root in the Money state
    pub snapshot_coins: MerkleNode,
    /// Snapshotted SMT root in the Money state
    pub snapshot_nulls: pallas::Base,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
// ANCHOR: dao-vote-params
/// Parameters for `Dao::Vote`
pub struct DaoVoteParams {
    /// Token commitment for the vote inputs
    pub token_commit: pallas::Base,
    /// Proposal bulla being voted on
    pub proposal_bulla: DaoProposalBulla,
    /// Commitment for yes votes
    pub yes_vote_commit: pallas::Point,
    /// Encrypted note
    pub note: ElGamalEncryptedNote<4>,
    /// Inputs for the vote
    pub inputs: Vec<DaoVoteParamsInput>,
}
// ANCHOR_END: dao-vote-params

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
// ANCHOR: dao-vote-params-input
/// Input for a DAO proposal vote
pub struct DaoVoteParamsInput {
    /// Vote commitment
    pub vote_commit: pallas::Point,
    /// Vote nullifier
    pub vote_nullifier: Nullifier,
    /// Public key used for signing
    pub signature_public: PublicKey,
}
// ANCHOR_END: dao-vote-params-input

/// State update for `Dao::Vote`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoVoteUpdate {
    /// The proposal bulla being voted on
    pub proposal_bulla: DaoProposalBulla,
    /// The updated proposal metadata
    pub proposal_metadata: DaoProposalMetadata,
    /// Vote nullifiers,
    pub vote_nullifiers: Vec<Nullifier>,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
// ANCHOR: dao-blind-aggregate-vote
/// Represents a single or multiple blinded votes.
/// These can be summed together.
pub struct DaoBlindAggregateVote {
    /// Weighted vote commit
    pub yes_vote_commit: pallas::Point,
    /// All value staked in the vote
    pub all_vote_commit: pallas::Point,
}
// ANCHOR_END: dao-blind-aggregate-vote

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

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
// ANCHOR: dao-exec-params
/// Parameters for `Dao::Exec`
pub struct DaoExecParams {
    /// The proposal bulla
    pub proposal_bulla: DaoProposalBulla,
    /// The proposal auth calls
    pub proposal_auth_calls: Vec<DaoAuthCall>,
    /// Aggregated blinds for the vote commitments
    pub blind_total_vote: DaoBlindAggregateVote,
    /// Flag indicating if its early execution
    pub early_exec: bool,
    /// Public key for the signature.
    /// The signature ensures this DAO::exec call cannot be modified with other calls.
    pub signature_public: PublicKey,
}
// ANCHOR_END: dao-exec-params

/// State update for `Dao::Exec`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoExecUpdate {
    /// The proposal bulla
    pub proposal_bulla: DaoProposalBulla,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
// ANCHOR: dao-auth_xfer-params
/// Parameters for `Dao::AuthMoneyTransfer`
pub struct DaoAuthMoneyTransferParams {
    pub enc_attrs: Vec<ElGamalEncryptedNote<5>>,
    pub dao_change_attrs: ElGamalEncryptedNote<3>,
}
// ANCHOR_END: dao-auth_xfer-params
