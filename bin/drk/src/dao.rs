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

use darkfi_sdk::{
    crypto::{SecretKey, TokenId},
    incrementalmerkletree::Position,
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
/// Parameters representing a DAO to be initialized
pub struct DaoParams {
    /// The minimum amount of governance tokens needed to open a proposal
    pub proposer_limit: u64,
    /// Minimal threshold of participating total tokens needed for a proposal to pass
    pub quorum: u64,
    /// The ratio of winning/total votes needed for a proposal to pass
    pub approval_ratio_base: u64,
    pub approval_ratio_quot: u64,
    /// DAO's governance token ID
    pub gov_token_id: TokenId,
    /// Secret key for the DAO
    pub secret_key: SecretKey,
    /// DAO bulla blind
    pub bulla_blind: pallas::Base,
}

#[derive(Debug, Clone)]
/// Parameters representing an intialized DAO, optionally deployed on-chain
pub struct Dao {
    /// Named identifier for the DAO
    pub name: String,
    /// The minimum amount of governance tokens needed to open a proposal
    pub proposer_limit: u64,
    /// Minimal threshold of participating total tokens needed for a proposal to pass
    pub quorum: u64,
    /// The ratio of winning/total votes needed for a proposal to pass
    pub approval_ratio_base: u64,
    pub approval_ratio_quot: u64,
    /// DAO's governance token ID
    pub gov_token_id: TokenId,
    /// Secret key for the DAO
    pub secret_key: SecretKey,
    /// DAO bulla blind
    pub bulla_blind: pallas::Base,
    /// Leaf position of the DAO in the Merkle tree of DAOs
    pub leaf_position: Option<Position>,
    /// The transaction hash where the DAO was deployed
    pub tx_hash: Option<blake3::Hash>,
    /// The call index in the transaction where the DAO was deployed
    pub call_index: Option<u32>,
}
