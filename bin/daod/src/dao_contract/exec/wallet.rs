use log::debug;
use rand::rngs::OsRng;

use halo2_proofs::circuit::Value;
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::Field, Curve},
    pallas,
};

use darkfi::{
    crypto::Proof,
    zk::vm::{Witness, ZkCircuit},
};

use crate::{
    dao_contract::{
        exec::validate::CallData,
        propose::wallet::{DaoParams, Proposal},
    },
    demo::{FuncCall, ZkContractInfo, ZkContractTable},
    util::poseidon_hash,
};

pub struct Builder {
    pub proposal: Proposal,
    pub dao: DaoParams,
    pub win_votes: u64,
    pub total_votes: u64,
    pub win_votes_blind: pallas::Scalar,
    pub total_votes_blind: pallas::Scalar,
    pub user_serial: pallas::Base,
    pub user_coin_blind: pallas::Base,
    pub dao_serial: pallas::Base,
    pub dao_coin_blind: pallas::Base,
    pub input_value: u64,
    pub input_value_blind: pallas::Scalar,
    pub hook_dao_exec: pallas::Base,
}

impl Builder {
    pub fn build(self, zk_bins: &ZkContractTable) -> FuncCall {
        debug!(target: "dao_contract::exec::wallet::Builder", "build()");
        let mut proofs = vec![];

        let proposal_dest_coords = self.proposal.dest.0.to_affine().coordinates().unwrap();
        let proposal_dest_x = *proposal_dest_coords.x();
        let proposal_dest_y = *proposal_dest_coords.y();

        let proposal_amount = pallas::Base::from(self.proposal.amount);

        let dao_proposer_limit = pallas::Base::from(self.dao.proposer_limit);
        let dao_quorum = pallas::Base::from(self.dao.quorum);
        let dao_approval_ratio = pallas::Base::from(self.dao.approval_ratio);

        let dao_pubkey_coords = self.dao.public_key.0.to_affine().coordinates().unwrap();
        let dao_public_x = *dao_pubkey_coords.x();
        let dao_public_y = *dao_pubkey_coords.x();

        let user_spend_hook = pallas::Base::from(0);
        let user_data = pallas::Base::from(0);
        let input_value = pallas::Base::from(self.input_value);
        let change = input_value - proposal_amount;

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

        let proposal_bulla = poseidon_hash::<8>([
            proposal_dest_x,
            proposal_dest_y,
            proposal_amount,
            self.proposal.serial,
            self.proposal.token_id,
            dao_bulla,
            self.proposal.blind,
            // @tmp-workaround
            self.proposal.blind,
        ]);

        let coin_0 = poseidon_hash::<8>([
            proposal_dest_x,
            proposal_dest_y,
            proposal_amount,
            self.proposal.token_id,
            self.proposal.serial,
            user_spend_hook,
            user_data,
            self.proposal.blind,
        ]);

        let coin_1 = poseidon_hash::<8>([
            dao_public_x,
            dao_public_y,
            change,
            self.dao.gov_token_id,
            self.dao_serial,
            self.hook_dao_exec,
            proposal_bulla,
            self.dao_coin_blind,
        ]);

        let zk_info = zk_bins.lookup(&"dao-exec".to_string()).unwrap();
        let zk_info = if let ZkContractInfo::Binary(info) = zk_info {
            info
        } else {
            panic!("Not binary info")
        };

        let zk_bin = zk_info.bincode.clone();

        let prover_witnesses = vec![
            // proposal params
            Witness::Base(Value::known(proposal_dest_x)),
            Witness::Base(Value::known(proposal_dest_y)),
            Witness::Base(Value::known(proposal_amount)),
            Witness::Base(Value::known(self.proposal.serial)),
            Witness::Base(Value::known(self.proposal.token_id)),
            Witness::Base(Value::known(self.proposal.blind)),
            // DAO params
            Witness::Base(Value::known(dao_proposer_limit)),
            Witness::Base(Value::known(dao_quorum)),
            Witness::Base(Value::known(dao_approval_ratio)),
            Witness::Base(Value::known(self.dao.gov_token_id)),
            Witness::Base(Value::known(dao_public_x)),
            Witness::Base(Value::known(dao_public_y)),
            Witness::Base(Value::known(self.dao.bulla_blind)),
            // votes
            Witness::Base(Value::known(pallas::Base::from(self.win_votes))),
            Witness::Base(Value::known(pallas::Base::from(self.total_votes))),
            Witness::Scalar(Value::known(self.win_votes_blind)),
            Witness::Scalar(Value::known(self.total_votes_blind)),
            // outputs + inputs
            Witness::Base(Value::known(self.user_serial)),
            Witness::Base(Value::known(self.user_coin_blind)),
            Witness::Base(Value::known(self.dao_serial)),
            Witness::Base(Value::known(self.dao_coin_blind)),
            Witness::Base(Value::known(input_value)),
            Witness::Scalar(Value::known(self.input_value_blind)),
            // misc
            Witness::Base(Value::known(self.hook_dao_exec)),
            Witness::Base(Value::known(user_spend_hook)),
            Witness::Base(Value::known(user_data)),
        ];

        let public_inputs = vec![proposal_bulla, coin_0, coin_1];

        let circuit = ZkCircuit::new(prover_witnesses, zk_bin);
        debug!(target: "example_contract::foo::wallet::Builder", "input_proof Proof::create()");
        let proving_key = &zk_info.proving_key;
        let input_proof = Proof::create(proving_key, &[circuit], &public_inputs, &mut OsRng)
            .expect("DAO::exec() proving error!)");
        proofs.push(input_proof);

        let call_data = CallData { proposal: proposal_bulla, coin_0, coin_1 };

        FuncCall {
            contract_id: "DAO".to_string(),
            func_id: "DAO::exec()".to_string(),
            call_data: Box::new(call_data),
            proofs,
        }
    }
}
