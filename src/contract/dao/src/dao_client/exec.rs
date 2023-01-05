/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use darkfi_sdk::{
    crypto::{
        pedersen::pedersen_commitment_u64, poseidon_hash, MerkleNode, PublicKey, SecretKey, TokenId,
    },
    incrementalmerkletree,
    incrementalmerkletree::{bridgetree::BridgeTree, Hashable, Tree},
    pasta::{
        arithmetic::CurveAffine,
        group::{
            ff::{Field, PrimeField},
            Curve,
        },
        pallas,
    },
};
use darkfi_serial::{SerialDecodable, SerialEncodable};
use halo2_proofs::circuit::Value;
use log::{debug, info};
use rand::rngs::OsRng;

use darkfi::{
    zk::{
        proof::{Proof, ProvingKey},
        vm::ZkCircuit,
        vm_stack::Witness,
    },
    zkas::ZkBinary,
    Error, Result,
};

use super::{propose::Proposal, Dao};
use crate::dao_model::DaoExecParams;

pub struct Builder {
    pub proposal: Proposal,
    pub dao: Dao,
    pub yes_votes_value: u64,
    pub all_votes_value: u64,
    pub yes_votes_blind: pallas::Scalar,
    pub all_votes_blind: pallas::Scalar,
    pub user_serial: pallas::Base,
    pub user_coin_blind: pallas::Base,
    pub dao_serial: pallas::Base,
    pub dao_coin_blind: pallas::Base,
    pub input_value: u64,
    pub input_value_blind: pallas::Scalar,
    pub hook_dao_exec: pallas::Base,
    pub signature_secret: SecretKey,
}

impl Builder {
    pub fn build(
        self,
        exec_zkbin: &ZkBinary,
        exec_pk: &ProvingKey,
    ) -> Result<(DaoExecParams, Vec<Proof>)> {
        debug!(target: "dao", "build()");
        let mut proofs = vec![];

        let (proposal_dest_x, proposal_dest_y) = self.proposal.dest.xy();

        let proposal_amount = pallas::Base::from(self.proposal.amount);

        let dao_proposer_limit = pallas::Base::from(self.dao.proposer_limit);
        let dao_quorum = pallas::Base::from(self.dao.quorum);
        let dao_approval_ratio_quot = pallas::Base::from(self.dao.approval_ratio_quot);
        let dao_approval_ratio_base = pallas::Base::from(self.dao.approval_ratio_base);

        let (dao_pub_x, dao_pub_y) = self.dao.public_key.xy();

        let user_spend_hook = pallas::Base::from(0);
        let user_data = pallas::Base::from(0);
        let input_value = pallas::Base::from(self.input_value);
        let change = input_value - proposal_amount;

        let dao_bulla = poseidon_hash::<8>([
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio_quot,
            dao_approval_ratio_base,
            self.dao.gov_token_id.inner(),
            dao_pub_x,
            dao_pub_y,
            self.dao.bulla_blind,
        ]);

        let proposal_bulla = poseidon_hash::<8>([
            proposal_dest_x,
            proposal_dest_y,
            proposal_amount,
            self.proposal.serial,
            self.proposal.token_id.inner(),
            dao_bulla,
            self.proposal.blind,
            // @tmp-workaround
            self.proposal.blind,
        ]);

        let coin_0 = poseidon_hash::<8>([
            proposal_dest_x,
            proposal_dest_y,
            proposal_amount,
            self.proposal.token_id.inner(),
            self.proposal.serial,
            user_spend_hook,
            user_data,
            self.proposal.blind,
        ]);

        let coin_1 = poseidon_hash::<8>([
            dao_pub_x,
            dao_pub_y,
            change,
            self.proposal.token_id.inner(),
            self.dao_serial,
            self.hook_dao_exec,
            dao_bulla,
            self.dao_coin_blind,
        ]);

        let yes_votes_commit = pedersen_commitment_u64(self.yes_votes_value, self.yes_votes_blind);
        let yes_votes_commit_coords = yes_votes_commit.to_affine().coordinates().unwrap();

        let all_votes_commit = pedersen_commitment_u64(self.all_votes_value, self.all_votes_blind);
        let all_votes_commit_coords = all_votes_commit.to_affine().coordinates().unwrap();

        let input_value_commit = pedersen_commitment_u64(self.input_value, self.input_value_blind);
        let input_value_commit_coords = input_value_commit.to_affine().coordinates().unwrap();

        /*
        let zk_info = zk_bins.lookup(&"dao-exec".to_string()).unwrap();
        let zk_info = if let ZkContractInfo::Binary(info) = zk_info {
            info
        } else {
            panic!("Not binary info")
        };

        let zk_bin = zk_info.bincode.clone();
        */

        let prover_witnesses = vec![
            //
            // proposal params
            Witness::Base(Value::known(proposal_dest_x)),
            Witness::Base(Value::known(proposal_dest_y)),
            Witness::Base(Value::known(proposal_amount)),
            Witness::Base(Value::known(self.proposal.serial)),
            Witness::Base(Value::known(self.proposal.token_id.inner())),
            Witness::Base(Value::known(self.proposal.blind)),
            // DAO params
            Witness::Base(Value::known(dao_proposer_limit)),
            Witness::Base(Value::known(dao_quorum)),
            Witness::Base(Value::known(dao_approval_ratio_quot)),
            Witness::Base(Value::known(dao_approval_ratio_base)),
            Witness::Base(Value::known(self.dao.gov_token_id.inner())),
            Witness::Base(Value::known(dao_pub_x)),
            Witness::Base(Value::known(dao_pub_y)),
            Witness::Base(Value::known(self.dao.bulla_blind)),
            // votes
            Witness::Base(Value::known(pallas::Base::from(self.yes_votes_value))),
            Witness::Base(Value::known(pallas::Base::from(self.all_votes_value))),
            Witness::Scalar(Value::known(self.yes_votes_blind)),
            Witness::Scalar(Value::known(self.all_votes_blind)),
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

        let public_inputs = vec![
            proposal_bulla,
            coin_0,
            coin_1,
            *yes_votes_commit_coords.x(),
            *yes_votes_commit_coords.y(),
            *all_votes_commit_coords.x(),
            *all_votes_commit_coords.y(),
            *input_value_commit_coords.x(),
            *input_value_commit_coords.y(),
            self.hook_dao_exec,
            user_spend_hook,
            user_data,
        ];

        let circuit = ZkCircuit::new(prover_witnesses, exec_zkbin.clone());
        let input_proof = Proof::create(&exec_pk, &[circuit], &public_inputs, &mut OsRng)
            .expect("DAO::exec() proving error!)");
        proofs.push(input_proof);

        let params = DaoExecParams {
            proposal: proposal_bulla,
            coin_0,
            coin_1,
            yes_votes_commit,
            all_votes_commit,
            input_value_commit,
        };

        Ok((params, proofs))
    }
}
