use halo2_proofs::circuit::Value;
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::Field, Curve},
    pallas,
};
use rand::rngs::OsRng;

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
    util::poseidon_hash,
};

pub struct Input {
    pub secret: SecretKey,
    pub note: money_contract::transfer::wallet::Note,
    pub leaf_position: incrementalmerkletree::Position,
    pub merkle_path: Vec<MerkleNode>,
}

pub struct Proposal {
    pub dest: PublicKey,
    pub amount: u64,
    pub serial: pallas::Base,
    pub token_id: pallas::Base,
    pub blind: pallas::Base,
}

pub struct DaoParams {
    pub proposer_limit: u64,
    pub quorum: u64,
    pub approval_ratio: u64,
    pub gov_token_id: pallas::Base,
    pub public_key: PublicKey,
    pub bulla_blind: pallas::Base,
}

pub struct Builder {
    pub inputs: Vec<Input>,
    pub proposal: Proposal,
    pub dao: DaoParams,
}

impl Builder {
    pub fn build(self, zk_bins: &ZkContractTable) -> FuncCall {
        let dao_proposer_limit = pallas::Base::from(self.dao.proposer_limit);
        let dao_quorum = pallas::Base::from(self.dao.quorum);
        let dao_approval_ratio = pallas::Base::from(self.dao.approval_ratio);

        let dao_pubkey_coords = self.dao.public_key.0.to_affine().coordinates().unwrap();
        let dao_public_x = *dao_pubkey_coords.x();
        let dao_public_y = *dao_pubkey_coords.x();

        let dao_bulla = poseidon_hash::<8>([
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio,
            self.dao.gov_token_id,
            dao_public_x,
            dao_public_y,
            self.dao.bulla_blind,
            // @tmp-workaround
            self.dao.bulla_blind,
        ]);

        let zk_info = zk_bins.lookup(&"dao-propose-main".to_string()).unwrap();
        let zk_info = if let ZkContractInfo::Binary(info) = zk_info {
            info
        } else {
            panic!("Not binary info")
        };
        let zk_bin = zk_info.bincode.clone();
        let prover_witnesses = vec![
            Witness::Base(Value::known(dao_proposer_limit)),
            Witness::Base(Value::known(dao_quorum)),
            Witness::Base(Value::known(dao_approval_ratio)),
            Witness::Base(Value::known(self.dao.gov_token_id)),
            Witness::Base(Value::known(dao_public_x)),
            Witness::Base(Value::known(dao_public_y)),
            Witness::Base(Value::known(self.dao.bulla_blind)),
        ];
        let public_inputs = vec![dao_bulla];
        let circuit = ZkCircuit::new(prover_witnesses, zk_bin);

        let proving_key = &zk_info.proving_key;
        let main_proof = Proof::create(proving_key, &[circuit], &public_inputs, &mut OsRng)
            .expect("DAO::propose() proving error!");

        let call_data = CallData { dao_bulla };

        FuncCall {
            contract_id: "DAO".to_string(),
            func_id: "DAO::propose()".to_string(),
            call_data: Box::new(call_data),
            proofs: vec![main_proof],
        }
    }
}
