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
    group::{ff::Field, Curve, Group},
    pallas,
};
use std::any::{Any, TypeId};

use crate::{
    dao_contract::{DaoBulla, State as DaoState},
    demo::{CallDataBase, StateRegistry, Transaction},
    money_contract::state::State as MoneyState,
    note::EncryptedNote2,
};

pub struct CallData {
    pub header: Header,
    pub inputs: Vec<Input>,
    pub signatures: Vec<schnorr::Signature>,
}

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

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct Header {
    pub enc_note: EncryptedNote2,
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct Input {
    pub value_commit: pallas::Point,
    pub merkle_root: MerkleNode,
    pub signature_public: PublicKey,
}
