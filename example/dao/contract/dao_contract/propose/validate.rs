use std::any::{Any, TypeId};

use darkfi_serial::{Encodable, SerialDecodable, SerialEncodable};
use log::error;
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{Curve, Group},
    pallas,
};

use darkfi::{
    crypto::{keypair::PublicKey, merkle_node::MerkleNode, types::DrkCircuitField},
    Error as DarkFiError,
};

use crate::{
    contract::{
        dao_contract, dao_contract::State as DaoState, money_contract,
        money_contract::state::State as MoneyState,
    },
    note::EncryptedNote2,
    util::{CallDataBase, StateRegistry, Transaction, UpdateBase},
};

// used for debugging
// const TARGET: &str = "dao_contract::propose::validate::state_transition()";

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("Invalid input merkle root")]
    InvalidInputMerkleRoot,

    #[error("Invalid DAO merkle root")]
    InvalidDaoMerkleRoot,

    #[error("DarkFi error: {0}")]
    DarkFiError(String),
}
type Result<T> = std::result::Result<T, Error>;

impl From<DarkFiError> for Error {
    fn from(err: DarkFiError) -> Self {
        Self::DarkFiError(err.to_string())
    }
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct CallData {
    pub header: Header,
    pub inputs: Vec<Input>,
}

impl CallDataBase for CallData {
    fn zk_public_values(&self) -> Vec<(String, Vec<DrkCircuitField>)> {
        let mut zk_publics = Vec::new();
        let mut total_funds_commit = pallas::Point::identity();

        assert!(!self.inputs.is_empty(), "inputs length cannot be zero");
        for input in &self.inputs {
            total_funds_commit += input.value_commit;
            let value_coords = input.value_commit.to_affine().coordinates().unwrap();

            let sigpub_coords = input.signature_public.0.to_affine().coordinates().unwrap();

            zk_publics.push((
                "dao-propose-burn".to_string(),
                vec![
                    *value_coords.x(),
                    *value_coords.y(),
                    self.header.token_commit,
                    input.merkle_root.0,
                    *sigpub_coords.x(),
                    *sigpub_coords.y(),
                ],
            ));
        }

        let total_funds_coords = total_funds_commit.to_affine().coordinates().unwrap();
        zk_publics.push((
            "dao-propose-main".to_string(),
            vec![
                self.header.token_commit,
                self.header.dao_merkle_root.0,
                self.header.proposal_bulla,
                *total_funds_coords.x(),
                *total_funds_coords.y(),
            ],
        ));

        zk_publics
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn signature_public_keys(&self) -> Vec<PublicKey> {
        let mut signature_public_keys = vec![];
        for input in self.inputs.clone() {
            signature_public_keys.push(input.signature_public);
        }
        signature_public_keys
    }

    fn encode_bytes(
        &self,
        mut writer: &mut dyn std::io::Write,
    ) -> core::result::Result<usize, std::io::Error> {
        self.encode(&mut writer)
    }
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct Header {
    pub dao_merkle_root: MerkleNode,
    pub token_commit: pallas::Base,
    pub proposal_bulla: pallas::Base,
    pub enc_note: EncryptedNote2,
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct Input {
    pub value_commit: pallas::Point,
    pub merkle_root: MerkleNode,
    pub signature_public: PublicKey,
}

pub fn state_transition(
    states: &StateRegistry,
    func_call_index: usize,
    parent_tx: &Transaction,
) -> Result<Box<dyn UpdateBase + Send>> {
    let func_call = &parent_tx.func_calls[func_call_index];
    let call_data = func_call.call_data.as_any();

    assert_eq!((*call_data).type_id(), TypeId::of::<CallData>());
    let call_data = call_data.downcast_ref::<CallData>();

    // This will be inside wasm so unwrap is fine.
    let call_data = call_data.unwrap();

    // Check the merkle roots for the input coins are valid
    for input in &call_data.inputs {
        let money_state = states.lookup::<MoneyState>(*money_contract::CONTRACT_ID).unwrap();
        if !money_state.is_valid_merkle(&input.merkle_root) {
            return Err(Error::InvalidInputMerkleRoot)
        }
    }

    let state = states.lookup::<DaoState>(*dao_contract::CONTRACT_ID).unwrap();

    // Is the DAO bulla generated in the ZK proof valid
    if !state.is_valid_dao_merkle(&call_data.header.dao_merkle_root) {
        return Err(Error::InvalidDaoMerkleRoot)
    }

    // TODO: look at gov tokens avoid using already spent ones
    // Need to spend original coin and generate 2 nullifiers?

    Ok(Box::new(Update { proposal_bulla: call_data.header.proposal_bulla }))
}

#[derive(Clone)]
pub struct Update {
    pub proposal_bulla: pallas::Base,
}

impl UpdateBase for Update {
    fn apply(self: Box<Self>, states: &mut StateRegistry) {
        let state = states.lookup_mut::<DaoState>(*dao_contract::CONTRACT_ID).unwrap();
        state.add_proposal_bulla(self.proposal_bulla);
    }
}
