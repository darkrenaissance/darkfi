use pasta_curves::{arithmetic::CurveAffine, group::Curve, pallas};

use darkfi::{crypto::types::DrkCircuitField, Error as DarkFiError};

use std::any::{Any, TypeId};

use crate::demo::{CallDataBase, StateRegistry, Transaction, UpdateBase};

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("ValueExists")]
    ValueExists,

    #[error("DarkFi error: {0}")]
    DarkFiError(String),
}

impl From<DarkFiError> for Error {
    fn from(err: DarkFiError) -> Self {
        Self::DarkFiError(err.to_string())
    }
}

pub struct CallData {
    pub proposal: pallas::Base,
    pub coin_0: pallas::Base,
    pub coin_1: pallas::Base,
    pub win_votes_commit: pallas::Point,
    pub total_votes_commit: pallas::Point,
    pub input_value_commit: pallas::Point,
}

impl CallDataBase for CallData {
    fn zk_public_values(&self) -> Vec<(String, Vec<DrkCircuitField>)> {
        let win_votes_coords = self.win_votes_commit.to_affine().coordinates().unwrap();
        let win_votes_commit_x = *win_votes_coords.x();
        let win_votes_commit_y = *win_votes_coords.y();

        let total_votes_coords = self.total_votes_commit.to_affine().coordinates().unwrap();
        let total_votes_commit_x = *total_votes_coords.x();
        let total_votes_commit_y = *total_votes_coords.y();

        let input_value_coords = self.input_value_commit.to_affine().coordinates().unwrap();
        let input_value_commit_x = *input_value_coords.x();
        let input_value_commit_y = *input_value_coords.y();

        vec![(
            "dao-exec".to_string(),
            vec![
                self.proposal,
                self.coin_0,
                self.coin_1,
                win_votes_commit_x,
                win_votes_commit_y,
                total_votes_commit_x,
                total_votes_commit_y,
                input_value_commit_x,
                input_value_commit_y,
                *super::FUNC_ID,
                pallas::Base::from(0),
                pallas::Base::from(0),
            ],
        )]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub fn state_transition(
    states: &StateRegistry,
    func_call_index: usize,
    parent_tx: &Transaction,
) -> Result<Box<dyn UpdateBase>> {
    let func_call = &parent_tx.func_calls[func_call_index];
    let call_data = func_call.call_data.as_any();

    assert_eq!((&*call_data).type_id(), TypeId::of::<CallData>());
    let call_data = call_data.downcast_ref::<CallData>();

    // This will be inside wasm so unwrap is fine.
    let call_data = call_data.unwrap();

    // Enforce tx has correct format:
    // 1. There should only be 2 calldata's
    // 2. func_call_index == 1
    // 3. First item should be a Money::transfer() calldata
    // 4. Money::transfer() has exactly 2 outputs

    // Checks:

    // 1. Check both coins in Money::transfer() are equal to our coin_0, coin_1
    // 2. sum of Money::transfer() calldata input_value_commits == our input value commit
    // 3. get the ProposalVote from DAO::State
    // 4. check win/total_vote_commit is the same as in ProposalVote

    // We need the proposal in here
    Ok(Box::new(Update {}))
}

#[derive(Clone)]
pub struct Update {}

impl UpdateBase for Update {
    fn apply(mut self: Box<Self>, states: &mut StateRegistry) {
        // Delete the ProposalVotes from DAO::State hashmap
    }
}
