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

//! This module implements the client-side API for this contract's interaction.
//! What we basically do here is implement an API that creates the necessary
//! structures and is able to export them to create a DarkFi transaction
//! object that can be broadcasted to the network.
//!
//! Note that this API does not involve any wallet interaction, but only takes
//! the necessary objects provided by the caller. This is intentional, so we
//! are able to abstract away any wallet interfaces to client implementations.

use darkfi_money_contract::client::{MoneyNote, OwnCoin};
use darkfi_sdk::{
    crypto::{pasta_prelude::Field, Coin, MerklePosition, Nullifier, SecretKey, DARK_TOKEN_ID},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::model::ZERO;

/// Common functions
pub(crate) mod common;

/// `Consensus::GenesisStakeV1` API
pub mod genesis_stake_v1;

/// `Consensus::StakeV1` API
pub mod stake_v1;

/// Proposal transaction building API.
/// This transaction is a chain of `Consensus::ProposalBurnV1`, `Consensus::ProposalRewardV1`
/// and `Consensus::ProposalMintV1` contract calls.
pub mod proposal_v1;

/// `Consensus::UnstakeV1` API
pub mod unstake_v1;

/// `ConsensusNote` holds the inner attributes of a `Coin`.
#[derive(Debug, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct ConsensusNote {
    /// Serial number of the coin, used for the nullifier
    pub serial: pallas::Base,
    /// Value of the coin
    pub value: u64,
    /// Epoch the coin was minted
    pub epoch: u64,
    /// Blinding factor for the coin bulla
    pub coin_blind: pallas::Base,
    /// Blinding factor for the value pedersen commitment
    pub value_blind: pallas::Scalar,
}

impl From<ConsensusNote> for MoneyNote {
    fn from(consensus_note: ConsensusNote) -> Self {
        MoneyNote {
            serial: consensus_note.serial,
            value: consensus_note.value,
            token_id: *DARK_TOKEN_ID,
            spend_hook: ZERO,
            user_data: ZERO,
            coin_blind: consensus_note.coin_blind,
            value_blind: consensus_note.value_blind,
            token_blind: pallas::Scalar::zero(),
            memo: vec![],
        }
    }
}

/// `ConsensusOwnCoin` is a representation of `Coin` with its respective metadata.
#[derive(Debug, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct ConsensusOwnCoin {
    /// The coin hash
    pub coin: Coin,
    /// The attached `ConsensusNote`
    pub note: ConsensusNote,
    /// Coin's secret key
    pub secret: SecretKey,
    /// Coin's nullifier
    pub nullifier: Nullifier,
    /// Coin's leaf position in the Merkle tree of coins
    pub leaf_position: MerklePosition,
}

impl From<ConsensusOwnCoin> for OwnCoin {
    fn from(consensus_own_coin: ConsensusOwnCoin) -> Self {
        OwnCoin {
            coin: consensus_own_coin.coin,
            note: consensus_own_coin.note.into(),
            secret: consensus_own_coin.secret,
            nullifier: consensus_own_coin.nullifier,
            leaf_position: consensus_own_coin.leaf_position,
        }
    }
}
