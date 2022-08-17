use crate::{
    dao_contract::{DaoBulla, State},
    demo::{CallDataBase, StateRegistry, Transaction},
};
use darkfi::crypto::{merkle_node::MerkleNode, types::DrkCircuitField};
use log::{debug, error};
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::Field, Curve},
    pallas,
};
use std::any::{Any, TypeId};

const TARGET: &str = "dao_contract::propose::validate::state_transition()";

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("Invalid DAO merkle root")]
    InvalidDaoMerkleRoot,
}
type Result<T> = std::result::Result<T, Error>;

pub struct CallData {
    pub dao_merkle_root: MerkleNode,
}

impl CallDataBase for CallData {
    fn zk_public_values(&self) -> Vec<Vec<DrkCircuitField>> {
        vec![vec![self.dao_merkle_root.0]]
    }

    fn zk_proof_addrs(&self) -> Vec<String> {
        vec!["dao-propose-main".to_string()]
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
    let func_call = &parent_tx.func_calls[func_call_index];
    let call_data = func_call.call_data.as_any();

    assert_eq!((&*call_data).type_id(), TypeId::of::<CallData>());
    let call_data = call_data.downcast_ref::<CallData>();

    // This will be inside wasm so unwrap is fine.
    let call_data = call_data.unwrap();

    let state = states.lookup::<State>(&"DAO".to_string()).unwrap();

    // Is the DAO bulla generated in the ZK proof valid
    if !state.is_valid_dao_merkle(&call_data.dao_merkle_root) {
        return Err(Error::InvalidDaoMerkleRoot)
    }

    Ok(Update {})
}

pub struct Update {}

pub fn apply(states: &mut StateRegistry, update: Update) {
    let state = states.lookup_mut::<State>(&"DAO".to_string()).unwrap();
}
