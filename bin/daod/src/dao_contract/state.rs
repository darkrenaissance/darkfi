use pasta_curves::pallas;
use std::any::{Any, TypeId};

use crate::{
    dao_contract::mint::CallData,
    demo::{StateRegistry, Transaction},
    Result,
};

#[derive(Clone)]
pub struct DaoBulla(pub pallas::Base);

/// This DAO state is for all DAOs on the network. There should only be a single instance.
pub struct State {
    dao_bullas: Vec<DaoBulla>,
}

impl State {
    pub fn new() -> Box<dyn Any> {
        Box::new(Self { dao_bullas: Vec::new() })
    }

    pub fn add_bulla(&mut self, bulla: DaoBulla) {
        self.dao_bullas.push(bulla);
    }
}

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
    //let state = states.lookup::<super::State>(&"dao_contract".to_string()).unwrap();
    let state = states.lookup::<State>(&"dao_contract".to_string()).unwrap();
    // Add dao_bulla to state.dao_bullas
    state.add_bulla(update.dao_bulla);
}
