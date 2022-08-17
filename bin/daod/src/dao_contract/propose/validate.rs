use crate::{
    dao_contract::{DaoBulla, State},
    demo::{CallDataBase, StateRegistry, Transaction},
};
use darkfi::{
    crypto::{
        keypair::PublicKey, merkle_node::MerkleNode, schnorr, schnorr::SchnorrPublic,
        types::DrkCircuitField, Proof,
    },
    util::serial::{Encodable, SerialDecodable, SerialEncodable, VarInt},
    Error as DarkFiError,
};
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

    #[error("Signature verification failed")]
    SignatureVerifyFailed,

    #[error("DarkFi error: {0}")]
    DarkFiError(String),
}
type Result<T> = std::result::Result<T, Error>;

impl From<DarkFiError> for Error {
    fn from(err: DarkFiError) -> Self {
        Self::DarkFiError(err.to_string())
    }
}

pub struct CallData {
    pub header: Header,
    pub inputs: Vec<Input>,
    pub signatures: Vec<schnorr::Signature>,
}

impl CallDataBase for CallData {
    fn zk_public_values(&self) -> Vec<Vec<DrkCircuitField>> {
        let total_funds_coords = self.header.total_funds_commit.to_affine().coordinates().unwrap();
        let total_funds_x = *total_funds_coords.x();
        let total_funds_y = *total_funds_coords.y();
        vec![
            // dao-propose-main proof
            vec![
                self.header.token_commit,
                self.header.dao_merkle_root.0,
                total_funds_x,
                total_funds_y,
            ],
        ]
    }

    fn zk_proof_addrs(&self) -> Vec<String> {
        vec!["dao-propose-main".to_string()]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct Header {
    pub dao_merkle_root: MerkleNode,
    pub token_commit: pallas::Base,
    // TODO: compute from sum of input commits
    pub total_funds_commit: pallas::Point,
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct Input {
    pub signature_public: PublicKey,
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
    if !state.is_valid_dao_merkle(&call_data.header.dao_merkle_root) {
        return Err(Error::InvalidDaoMerkleRoot)
    }

    // Verify the available signatures
    let mut unsigned_tx_data = vec![];
    call_data.header.encode(&mut unsigned_tx_data).expect("failed to encode data");
    call_data.inputs.encode(&mut unsigned_tx_data).expect("failed to encode inputs");
    func_call.proofs.encode(&mut unsigned_tx_data).expect("failed to encode proofs");

    for (i, (input, signature)) in
        call_data.inputs.iter().zip(call_data.signatures.iter()).enumerate()
    {
        let public = &input.signature_public;
        if !public.verify(&unsigned_tx_data[..], signature) {
            return Err(Error::SignatureVerifyFailed)
        }
    }

    Ok(Update {})
}

pub struct Update {}

pub fn apply(states: &mut StateRegistry, update: Update) {
    let state = states.lookup_mut::<State>(&"DAO".to_string()).unwrap();
}
