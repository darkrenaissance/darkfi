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

pub mod mint;
pub use mint::{make_mint_call, DaoInfo};

/// Provides core structs for DAO::propose()
///
/// * `DaoProposalInfo` is the main info about the proposal.
/// * `DaoProposeStakeInput` are the staking inputs used to meet the `proposer_limit` threshold.
/// * `DaoProposeCall` is what creates the call data used on chain.
/// * `DaoProposeNote` is the secret shared info transmitted between DAO members.
pub mod propose;
pub use propose::{DaoProposeCall, DaoProposeNote, DaoProposeStakeInput};

/// Provides core structs for DAO::vote()
///
/// * `DaoVoteInfo` is the main info about the vote.
/// * `DaoVoteStakeInput` are the staking inputs used in actual voting.
/// * `DaoVoteCall` is what creates the call data used on chain.
/// * `DaoVoteNote` is the secret shared info transmitted between DAO members.
pub mod vote;
pub use vote::{DaoVoteCall, DaoVoteInput, DaoVoteNote};

pub mod exec;
pub use exec::DaoExecCall;

// Wallet SQL table constant names. These have to represent the SQL schema.
pub const DAO_DAOS_TABLE: &str = "dao_daos";
pub const DAO_DAOS_COL_DAO_ID: &str = "dao_id";
pub const DAO_DAOS_COL_NAME: &str = "name";
pub const DAO_DAOS_COL_PROPOSER_LIMIT: &str = "proposer_limit";
pub const DAO_DAOS_COL_QUORUM: &str = "quorum";
pub const DAO_DAOS_COL_APPROVAL_RATIO_BASE: &str = "approval_ratio_base";
pub const DAO_DAOS_COL_APPROVAL_RATIO_QUOT: &str = "approval_ratio_quot";
pub const DAO_DAOS_COL_GOV_TOKEN_ID: &str = "gov_token_id";
pub const DAO_DAOS_COL_SECRET: &str = "secret";
pub const DAO_DAOS_COL_BULLA_BLIND: &str = "bulla_blind";
pub const DAO_DAOS_COL_LEAF_POSITION: &str = "leaf_position";
pub const DAO_DAOS_COL_TX_HASH: &str = "tx_hash";
pub const DAO_DAOS_COL_CALL_INDEX: &str = "call_index";

pub const DAO_TREES_TABLE: &str = "dao_trees";
pub const DAO_TREES_COL_DAOS_TREE: &str = "daos_tree";
pub const DAO_TREES_COL_PROPOSALS_TREE: &str = "proposals_tree";

pub const DAO_COINS_TABLE: &str = "dao_coins";
pub const DAO_COINS_COL_COIN_ID: &str = "coin_id";
pub const DAO_COINS_COL_DAO_ID: &str = "dao_id";

pub const DAO_PROPOSALS_TABLE: &str = "dao_proposals";
pub const DAO_PROPOSALS_COL_PROPOSAL_ID: &str = "proposal_id";
pub const DAO_PROPOSALS_COL_DAO_ID: &str = "dao_id";
pub const DAO_PROPOSALS_COL_RECV_PUBLIC: &str = "recv_public";
pub const DAO_PROPOSALS_COL_AMOUNT: &str = "amount";
pub const DAO_PROPOSALS_COL_SENDCOIN_TOKEN_ID: &str = "sendcoin_token_id";
pub const DAO_PROPOSALS_COL_BULLA_BLIND: &str = "bulla_blind";
pub const DAO_PROPOSALS_COL_LEAF_POSITION: &str = "leaf_position";
pub const DAO_PROPOSALS_COL_MONEY_SNAPSHOT_TREE: &str = "money_snapshot_tree";
pub const DAO_PROPOSALS_COL_TX_HASH: &str = "tx_hash";
pub const DAO_PROPOSALS_COL_CALL_INDEX: &str = "call_index";
pub const DAO_PROPOSALS_COL_OUR_VOTE_ID: &str = "our_vote_id";

pub const DAO_VOTES_TABLE: &str = "dao_votes";
pub const DAO_VOTES_COL_VOTE_ID: &str = "vote_id";
pub const DAO_VOTES_COL_PROPOSAL_ID: &str = "proposal_id";
pub const DAO_VOTES_COL_VOTE_OPTION: &str = "vote_option";
pub const DAO_VOTES_COL_YES_VOTE_BLIND: &str = "yes_vote_blind";
pub const DAO_VOTES_COL_ALL_VOTE_VALUE: &str = "all_vote_value";
pub const DAO_VOTES_COL_ALL_VOTE_BLIND: &str = "all_vote_blind";
pub const DAO_VOTES_COL_TX_HASH: &str = "tx_hash";
pub const DAO_VOTES_COL_CALL_INDEX: &str = "call_index";
