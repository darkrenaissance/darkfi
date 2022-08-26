use pasta_curves::pallas;

use darkfi::{
    crypto::types::DrkCircuitField,
    util::serial::{SerialDecodable, SerialEncodable},
    Error as DarkFiError,
};

use std::any::{Any, TypeId};

use crate::{
    demo::{CallDataBase, StateRegistry, Transaction, UpdateBase},
    example_contract::state::State,
};

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
    pub win_votes_commit_x: pallas::Base,
    pub win_votes_commit_y: pallas::Base,
    pub total_votes_commit_x: pallas::Base,
    pub total_votes_commit_y: pallas::Base,
    pub input_value_commit_x: pallas::Base,
    pub input_value_commit_y: pallas::Base,
}

impl CallDataBase for CallData {
    fn zk_public_values(&self) -> Vec<(String, Vec<DrkCircuitField>)> {
        vec![(
            "dao-exec".to_string(),
            vec![
                self.proposal,
                self.coin_0,
                self.coin_1,
                self.win_votes_commit_x,
                self.win_votes_commit_y,
                self.total_votes_commit_x,
                self.total_votes_commit_y,
                self.input_value_commit_x,
                self.input_value_commit_y,
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

    //let example_state = states.lookup::<State>(&"Example".to_string()).unwrap();

    //if example_state.public_exists(&call_data.header.public_c) {
    //    return Err(Error::ValueExists)
    //}

    Ok(Box::new(Update {}))
}

#[derive(Clone)]
pub struct Update {}

impl UpdateBase for Update {
    fn apply(mut self: Box<Self>, states: &mut StateRegistry) {}
}
