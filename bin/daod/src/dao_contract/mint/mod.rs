use std::any::Any;

use darkfi::crypto::types::DrkCircuitField;

use super::state::DaoBulla;
use crate::demo::CallDataBase;

pub mod builder;
pub mod validate;
pub use builder::Builder;

/// This is an anonymous contract function that mutates the internal DAO state.
///
/// Corresponds to `mint(proposer_limit, quorum, approval_ratio, dao_pubkey, dao_blind)`
///
/// The prover creates a `Builder`, which then constructs the `Tx` that the verifier can
/// check using `state_transition()`.
///
/// # Arguments
///
/// * `proposer_limit` - Number of governance tokens that holder must possess in order to
///   propose a new vote.
/// * `quorum` - Number of minimum votes that must be met for a proposal to pass.
/// * `approval_ratio` - Ratio of winning to total votes for a proposal to pass.
/// * `dao_pubkey` - Public key of the DAO for permissioned access. This can also be
///   shared publicly if you want a full decentralized DAO.
/// * `dao_blind` - Blinding factor for the DAO bulla.
///
/// # Example
///
/// ```rust
/// let dao_proposer_limit = 110;
/// let dao_quorum = 110;
/// let dao_approval_ratio = 2;
///
/// let builder = dao_contract::Mint::Builder(
///     dao_proposer_limit,
///     dao_quorum,
///     dao_approval_ratio,
///     gov_token_id,
///     dao_pubkey,
///     dao_blind
/// );
/// let tx = builder.build();
/// ```

pub struct CallData {
    pub dao_bulla: DaoBulla,
}

impl CallDataBase for CallData {
    fn zk_public_values(&self) -> Vec<Vec<DrkCircuitField>> {
        vec![vec![self.dao_bulla.0]]
    }

    fn zk_proof_addrs(&self) -> Vec<String> {
        vec!["dao-mint".to_string()]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
