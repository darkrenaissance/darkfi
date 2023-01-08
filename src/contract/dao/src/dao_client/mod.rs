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
pub use vote::{VoteCall, VoteInfo, VoteInput, VoteNote};

pub mod exec;
pub use exec::ExecCall;
