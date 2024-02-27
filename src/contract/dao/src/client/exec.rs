/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
        pasta_prelude::*, pedersen_commitment_u64, BaseBlind, PublicKey, ScalarBlind, SecretKey,
    },
    pasta::pallas,
};

use log::debug;
use rand::rngs::OsRng;

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};

use crate::model::{Dao, DaoBlindAggregateVote, DaoExecParams, DaoProposal, VecAuthCallCommit};

pub struct DaoExecCall {
    pub proposal: DaoProposal,
    pub dao: Dao,
    pub yes_vote_value: u64,
    pub all_vote_value: u64,
    pub yes_vote_blind: ScalarBlind,
    pub all_vote_blind: ScalarBlind,
    pub input_value: u64,
    pub input_value_blind: ScalarBlind,
    pub input_user_data_blind: BaseBlind,
    pub hook_dao_exec: pallas::Base,
    pub signature_secret: SecretKey,
}

impl DaoExecCall {
    pub fn make(
        self,
        exec_zkbin: &ZkBinary,
        exec_pk: &ProvingKey,
    ) -> Result<(DaoExecParams, Vec<Proof>)> {
        debug!(target: "dao", "build()");
        let mut proofs = vec![];

        let dao_proposer_limit = pallas::Base::from(self.dao.proposer_limit);
        let dao_quorum = pallas::Base::from(self.dao.quorum);
        let dao_approval_ratio_quot = pallas::Base::from(self.dao.approval_ratio_quot);
        let dao_approval_ratio_base = pallas::Base::from(self.dao.approval_ratio_base);

        let (dao_pub_x, dao_pub_y) = self.dao.public_key.xy();

        let dao_bulla = self.dao.to_bulla();
        assert_eq!(dao_bulla, self.proposal.dao_bulla);
        let proposal_bulla = self.proposal.to_bulla();

        let yes_vote_commit = pedersen_commitment_u64(self.yes_vote_value, self.yes_vote_blind);
        let yes_vote_commit_coords = yes_vote_commit.to_affine().coordinates().unwrap();

        let all_vote_commit = pedersen_commitment_u64(self.all_vote_value, self.all_vote_blind);
        let all_vote_commit_coords = all_vote_commit.to_affine().coordinates().unwrap();

        let proposal_auth_calls_commit = self.proposal.auth_calls.commit();

        let signature_public = PublicKey::from_secret(self.signature_secret);

        let prover_witnesses = vec![
            // proposal params
            Witness::Base(Value::known(proposal_auth_calls_commit)),
            Witness::Base(Value::known(pallas::Base::from(self.proposal.creation_day))),
            Witness::Base(Value::known(pallas::Base::from(self.proposal.duration_days))),
            Witness::Base(Value::known(self.proposal.user_data)),
            Witness::Base(Value::known(self.proposal.blind.inner())),
            // DAO params
            Witness::Base(Value::known(dao_proposer_limit)),
            Witness::Base(Value::known(dao_quorum)),
            Witness::Base(Value::known(dao_approval_ratio_quot)),
            Witness::Base(Value::known(dao_approval_ratio_base)),
            Witness::Base(Value::known(self.dao.gov_token_id.inner())),
            Witness::Base(Value::known(dao_pub_x)),
            Witness::Base(Value::known(dao_pub_y)),
            Witness::Base(Value::known(self.dao.bulla_blind.inner())),
            // votes
            Witness::Base(Value::known(pallas::Base::from(self.yes_vote_value))),
            Witness::Base(Value::known(pallas::Base::from(self.all_vote_value))),
            Witness::Scalar(Value::known(self.yes_vote_blind.inner())),
            Witness::Scalar(Value::known(self.all_vote_blind.inner())),
            // signature secret
            Witness::Base(Value::known(self.signature_secret.inner())),
        ];

        debug!(target: "dao", "proposal_bulla: {:?}", proposal_bulla);
        let public_inputs = vec![
            proposal_bulla.inner(),
            proposal_auth_calls_commit,
            *yes_vote_commit_coords.x(),
            *yes_vote_commit_coords.y(),
            *all_vote_commit_coords.x(),
            *all_vote_commit_coords.y(),
            signature_public.x(),
            signature_public.y(),
        ];
        //export_witness_json("witness.json", &prover_witnesses, &public_inputs);

        let circuit = ZkCircuit::new(prover_witnesses, exec_zkbin);
        let input_proof = Proof::create(exec_pk, &[circuit], &public_inputs, &mut OsRng)?;
        proofs.push(input_proof);

        let params = DaoExecParams {
            proposal_bulla,
            proposal_auth_calls: self.proposal.auth_calls,
            blind_total_vote: DaoBlindAggregateVote { yes_vote_commit, all_vote_commit },
            signature_public,
        };

        Ok((params, proofs))
    }
}
