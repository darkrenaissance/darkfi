use pasta_curves::pallas;

use darkfi::{
    crypto::types::DrkCircuitField,
    util::serial::{SerialDecodable, SerialEncodable},
    Error as DarkFiError,
};

use std::any::{Any, TypeId};

use crate::{
    demo::{CallDataBase, StateRegistry, Transaction},
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
    pub header: Header,
}

impl CallDataBase for CallData {
    fn zk_public_values(&self) -> Vec<(String, Vec<DrkCircuitField>)> {
        vec![("example-foo".to_string(), vec![self.header.public_c])]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct Header {
    pub public_c: pallas::Base,
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

    let example_state = states.lookup::<State>(&"Example".to_string()).unwrap();

    if example_state.public_exists(&call_data.header.public_c) {
        return Err(Error::ValueExists)
    }

    Ok(Update { public_value: call_data.header.public_c })
}

#[derive(Clone)]
pub struct Update {
    public_value: pallas::Base,
}

pub fn apply(states: &mut StateRegistry, update: Update) {
    let example_state = states.lookup_mut::<State>(&"Example".to_string()).unwrap();
    example_state.add_public_value(update.public_value);
}
