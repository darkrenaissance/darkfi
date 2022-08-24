use pasta_curves::pallas;
use std::any::{Any, TypeId};

use darkfi::crypto::types::DrkCircuitField;

use crate::{
    dao_contract::{DaoBulla, State},
    demo::{CallDataBase, StateRegistry, Transaction},
};

pub fn state_transition(
    states: &StateRegistry,
    func_call_index: usize,
    parent_tx: &Transaction,
) -> Result<Update> {
    let func_call = &parent_tx.func_calls[func_call_index];
    let call_data = func_call.call_data.as_any();

    assert_eq!((&*call_data).type_id(), TypeId::of::<CallData>());
    let call_data = call_data.downcast_ref::<CallData>();

    // This will be inside wasm so unwrap is fine.
    let call_data = call_data.unwrap();

    // Code goes here

    Ok(Update { dao_bulla: call_data.dao_bulla.clone() })
}

pub struct Update {
    pub dao_bulla: DaoBulla,
}

pub fn apply(states: &mut StateRegistry, update: Update) {
    // Lookup dao_contract state from registry
    let state = states.lookup_mut::<State>(&"DAO".to_string()).unwrap();
    // Add dao_bulla to state.dao_bullas
    state.add_dao_bulla(update.dao_bulla);
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("Malformed packet")]
    MalformedPacket,
}
type Result<T> = std::result::Result<T, Error>;

pub struct CallData {
    pub dao_bulla: DaoBulla,
}

impl CallDataBase for CallData {
    fn zk_public_values(&self) -> Vec<(String, Vec<DrkCircuitField>)> {
        vec![("dao-mint".to_string(), vec![self.dao_bulla.0])]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
