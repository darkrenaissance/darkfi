pub mod mint;
pub use mint::{make_mint_call, DaoInfo};

/// Provides core structs for DAO::propose()
///
/// * `ProposalInfo` is the main info about the proposal.
/// * `ProposeStakeInput` are the staking inputs used to meet the `proposer_limit` threshold.
/// * `ProposeCall` is what creates the call data used on chain.
/// * `ProposeNote` is the secret shared info transmitted between DAO members.
pub mod propose;
pub use propose::{ProposalInfo, ProposeCall, ProposeNote, ProposeStakeInput};

/// Provides core structs for DAO::vote()
///
/// * `VoteInfo` is the main info about the vote.
/// * `VoteStakeInput` are the staking inputs used in actual voting.
/// * `VoteCall` is what creates the call data used on chain.
/// * `VoteNote` is the secret shared info transmitted between DAO members.
pub mod vote;
pub use vote::{VoteCall, VoteInput, VoteNote};

pub mod exec;
pub use exec::ExecCall;

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
pub const DAO_PROPOSALS_COL_SERIAL: &str = "serial";
pub const DAO_PROPOSALS_COL_SENDCOIN_TOKEN_ID: &str = "sendcoin_token_id";
pub const DAO_PROPOSALS_COL_BULLA_BLIND: &str = "bulla_blind";
pub const DAO_PROPOSALS_COL_LEAF_POSITION: &str = "leaf_position";
pub const DAO_PROPOSALS_COL_TX_HASH: &str = "tx_hash";
pub const DAO_PROPOSALS_COL_CALL_INDEX: &str = "call_index";
pub const DAO_PROPOSALS_COL_OUR_VOTE_ID: &str = "our_vote_id";

pub const DAO_VOTES_TABLE: &str = "dao_votes";
pub const DAO_VOTES_COL_VOTE_ID: &str = "vote_id";
pub const DAO_VOTES_COL_PROPOSAL_ID: &str = "proposal_id";
pub const DAO_VOTES_COL_VOTE_OPTION: &str = "vote_option";
pub const DAO_VOTES_COL_TX_HASH: &str = "tx_hash";
pub const DAO_VOTES_COL_CALL_INDEX: &str = "call_index";
