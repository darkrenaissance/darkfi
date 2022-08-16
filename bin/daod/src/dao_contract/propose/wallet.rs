use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::Field, Curve},
    pallas,
};

use darkfi::{
    crypto::{
        burn_proof::create_burn_proof,
        keypair::{PublicKey, SecretKey},
        merkle_node::MerkleNode,
        mint_proof::create_mint_proof,
        proof::ProvingKey,
        schnorr::SchnorrSecret,
        types::{
            DrkCircuitField, DrkCoinBlind, DrkSerial, DrkSpendHook, DrkTokenId, DrkUserData,
            DrkUserDataBlind, DrkValueBlind,
        },
        Proof,
    },
    util::serial::{Encodable, SerialDecodable, SerialEncodable},
    zk::vm::{Witness, ZkCircuit},
};

use crate::{
    dao_contract::propose::validate::CallData,
    demo::{CallDataBase, FuncCall, ZkContractInfo, ZkContractTable},
    money_contract,
};

pub struct Input {
    //pub leaf_position: incrementalmerkletree::Position,
    //pub merkle_path: Vec<MerkleNode>,
    pub secret: SecretKey,
    pub note: money_contract::transfer::wallet::Note,
}

pub struct Proposal {
    pub dest: PublicKey,
    pub amount: u64,
    pub serial: pallas::Base,
    pub token_id: pallas::Base,
    pub blind: pallas::Base,
}

pub struct DaoParams {
    pub dao_proposer_limit: u64,
    pub dao_quorum: u64,
    pub dao_approval_ratio: u64,
    pub gov_token_id: pallas::Base,
    pub dao_public_key: PublicKey,
    pub dao_bulla_blind: pallas::Base,
}

pub struct Builder {
    pub inputs: Vec<Input>,
    pub proposal: Proposal,
    pub dao: DaoParams,
}

impl Builder {
    pub fn build(self, zk_bins: &ZkContractTable) -> FuncCall {
        let call_data = CallData {};
        FuncCall {
            contract_id: "DAO".to_string(),
            func_id: "DAO::propose()".to_string(),
            call_data: Box::new(call_data),
            proofs: vec![],
        }
    }
}
