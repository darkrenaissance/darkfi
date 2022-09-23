use crate::dao_contract::state::DaoBulla;

use darkfi::{
    crypto::{
        keypair::{PublicKey, SecretKey},
        util::poseidon_hash,
        Proof,
    },
    zk::vm::{Witness, ZkCircuit},
};
use halo2_proofs::circuit::Value;
use pasta_curves::{arithmetic::CurveAffine, group::Curve, pallas};
use rand::rngs::OsRng;

use crate::{
    dao_contract::{mint::validate::CallData, CONTRACT_ID},
    demo::{FuncCall, ZkContractInfo, ZkContractTable},
};

#[derive(Clone)]
pub struct DaoParams {
    pub proposer_limit: u64,
    pub quorum: u64,
    pub approval_ratio_quot: u64,
    pub approval_ratio_base: u64,
    pub gov_token_id: pallas::Base,
    pub public_key: PublicKey,
    pub bulla_blind: pallas::Base,
}

pub struct Builder {
    pub dao_proposer_limit: u64,
    pub dao_quorum: u64,
    pub dao_approval_ratio_quot: u64,
    pub dao_approval_ratio_base: u64,
    pub gov_token_id: pallas::Base,
    pub dao_pubkey: PublicKey,
    pub dao_bulla_blind: pallas::Base,
    pub _signature_secret: SecretKey,
}

impl Builder {
    /// Consumes self, and produces the function call
    pub fn build(self, zk_bins: &ZkContractTable) -> FuncCall {
        // Dao bulla
        let dao_proposer_limit = pallas::Base::from(self.dao_proposer_limit);
        let dao_quorum = pallas::Base::from(self.dao_quorum);
        let dao_approval_ratio_quot = pallas::Base::from(self.dao_approval_ratio_quot);
        let dao_approval_ratio_base = pallas::Base::from(self.dao_approval_ratio_base);

        let dao_pubkey_coords = self.dao_pubkey.0.to_affine().coordinates().unwrap();

        let dao_bulla = poseidon_hash::<8>([
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio_quot,
            dao_approval_ratio_base,
            self.gov_token_id,
            *dao_pubkey_coords.x(),
            *dao_pubkey_coords.y(),
            self.dao_bulla_blind,
        ]);
        let dao_bulla = DaoBulla(dao_bulla);

        // Now create the mint proof
        let zk_info = zk_bins.lookup(&"dao-mint".to_string()).unwrap();
        let zk_info = if let ZkContractInfo::Binary(info) = zk_info {
            info
        } else {
            panic!("Not binary info")
        };
        let zk_bin = zk_info.bincode.clone();
        let prover_witnesses = vec![
            Witness::Base(Value::known(dao_proposer_limit)),
            Witness::Base(Value::known(dao_quorum)),
            Witness::Base(Value::known(dao_approval_ratio_quot)),
            Witness::Base(Value::known(dao_approval_ratio_base)),
            Witness::Base(Value::known(self.gov_token_id)),
            Witness::Base(Value::known(*dao_pubkey_coords.x())),
            Witness::Base(Value::known(*dao_pubkey_coords.y())),
            Witness::Base(Value::known(self.dao_bulla_blind)),
        ];
        let public_inputs = vec![dao_bulla.0];
        let circuit = ZkCircuit::new(prover_witnesses, zk_bin);

        let proving_key = &zk_info.proving_key;
        let mint_proof = Proof::create(proving_key, &[circuit], &public_inputs, &mut OsRng)
            .expect("DAO::mint() proving error!");

        let call_data = CallData { dao_bulla };
        FuncCall {
            contract_id: *CONTRACT_ID,
            func_id: *super::FUNC_ID,
            call_data: Box::new(call_data),
            proofs: vec![mint_proof],
        }
    }
}
