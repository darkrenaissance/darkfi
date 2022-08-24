use darkfi::{crypto::types::DrkCircuitField, Error as DarkFiError};

use std::any::Any;

use crate::demo::{CallDataBase, StateRegistry, Transaction};

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("DarkFi error: {0}")]
    DarkFiError(String),
}

impl From<DarkFiError> for Error {
    fn from(err: DarkFiError) -> Self {
        Self::DarkFiError(err.to_string())
    }
}

pub struct CallData {}

impl CallDataBase for CallData {
    fn zk_public_values(&self) -> Vec<Vec<DrkCircuitField>> {
        vec![]
    }
    fn zk_proof_addrs(&self) -> Vec<String> {
        vec![]
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub fn state_transition(
    states: &StateRegistry,
    func_call_index: usize,
    parent_tx: &Transaction,
) -> Result<Update> {
    Ok(Update {})
}

#[derive(Clone)]
pub struct Update {}

pub fn apply(states: &mut StateRegistry, mut update: Update) {}
