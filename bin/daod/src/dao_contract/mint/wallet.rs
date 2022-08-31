use crate::dao_contract::state::DaoBulla;

use darkfi::{
    crypto::{keypair::PublicKey, util::poseidon_hash, Proof},
    zk::vm::{Witness, ZkCircuit},
};
use halo2_proofs::circuit::Value;
use pasta_curves::{arithmetic::CurveAffine, group::Curve, pallas};
use rand::rngs::OsRng;

use crate::{
    dao_contract::mint::validate::CallData,
    demo::{FuncCall, ZkContractInfo, ZkContractTable},
};

pub struct Builder {
    dao_proposer_limit: u64,
    dao_quorum: u64,
    dao_approval_ratio: u64,
    gov_token_id: pallas::Base,
    dao_pubkey: PublicKey,
    dao_bulla_blind: pallas::Base,
}

impl Builder {
    pub fn new(
        dao_proposer_limit: u64,
        dao_quorum: u64,
        dao_approval_ratio: u64,
        gov_token_id: pallas::Base,
        dao_pubkey: PublicKey,
        dao_bulla_blind: pallas::Base,
    ) -> Self {
        Self {
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio,
            gov_token_id,
            dao_pubkey,
            dao_bulla_blind,
        }
    }

    /// Consumes self, and produces the function call
    pub fn build(self, zk_bins: &ZkContractTable) -> FuncCall {
        // Dao bulla
        let dao_proposer_limit = pallas::Base::from(self.dao_proposer_limit);
        let dao_quorum = pallas::Base::from(self.dao_quorum);
        let dao_approval_ratio = pallas::Base::from(self.dao_approval_ratio);

        let dao_pubkey_coords = self.dao_pubkey.0.to_affine().coordinates().unwrap();

        let dao_bulla = poseidon_hash::<8>([
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio,
            self.gov_token_id,
            *dao_pubkey_coords.x(),
            *dao_pubkey_coords.y(),
            self.dao_bulla_blind,
            // @tmp-workaround
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
            Witness::Base(Value::known(dao_approval_ratio)),
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
            contract_id: "DAO".to_string(),
            func_id: "DAO::mint()".to_string(),
            call_data: Box::new(call_data),
            proofs: vec![mint_proof],
        }
    }
}
