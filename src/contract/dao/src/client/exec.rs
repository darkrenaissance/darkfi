/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
    crypto::{pasta_prelude::*, pedersen_commitment_u64, PublicKey, ScalarBlind, SecretKey},
    pasta::pallas,
};

use log::debug;
use rand::rngs::OsRng;

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    ClientFailed, Result,
};

use crate::{
    error::DaoError,
    model::{Dao, DaoBlindAggregateVote, DaoExecParams, DaoProposal, VecAuthCallCommit},
};

pub struct DaoExecCall {
    pub proposal: DaoProposal,
    pub dao: Dao,
    pub yes_vote_value: u64,
    pub all_vote_value: u64,
    pub yes_vote_blind: ScalarBlind,
    pub all_vote_blind: ScalarBlind,
    pub signature_secret: SecretKey,
    pub current_blockwindow: u64,
}

impl DaoExecCall {
    pub fn make(
        self,
        dao_exec_secret_key: &SecretKey,
        dao_early_exec_secret_key: &Option<SecretKey>,
        exec_zkbin: &ZkBinary,
        exec_pk: &ProvingKey,
    ) -> Result<(DaoExecParams, Vec<Proof>)> {
        debug!(target: "contract::dao::client::exec", "build()");
        let mut proofs = vec![];

        let dao_proposer_limit = pallas::Base::from(self.dao.proposer_limit);
        let dao_quorum = pallas::Base::from(self.dao.quorum);
        let dao_early_exec_quorum = pallas::Base::from(self.dao.early_exec_quorum);
        let dao_approval_ratio_quot = pallas::Base::from(self.dao.approval_ratio_quot);
        let dao_approval_ratio_base = pallas::Base::from(self.dao.approval_ratio_base);
        let (dao_notes_pub_x, dao_notes_pub_y) = self.dao.notes_public_key.xy();
        let (dao_proposer_pub_x, dao_proposer_pub_y) = self.dao.proposer_public_key.xy();
        let (dao_proposals_pub_x, dao_proposals_pub_y) = self.dao.proposals_public_key.xy();
        let (dao_votes_pub_x, dao_votes_pub_y) = self.dao.votes_public_key.xy();

        let dao_bulla = self.dao.to_bulla();
        if dao_bulla != self.proposal.dao_bulla {
            return Err(ClientFailed::VerifyError(DaoError::InvalidCalls.to_string()).into())
        }
        let proposal_bulla = self.proposal.to_bulla();

        let yes_vote_commit = pedersen_commitment_u64(self.yes_vote_value, self.yes_vote_blind);
        let yes_vote_commit_coords = yes_vote_commit.to_affine().coordinates().unwrap();

        let all_vote_commit = pedersen_commitment_u64(self.all_vote_value, self.all_vote_blind);
        let all_vote_commit_coords = all_vote_commit.to_affine().coordinates().unwrap();

        let proposal_auth_calls_commit = self.proposal.auth_calls.commit();

        let signature_public = PublicKey::from_secret(self.signature_secret);

        let current_blockwindow = pallas::Base::from(self.current_blockwindow);

        let mut prover_witnesses = vec![
            // Proposal params
            Witness::Base(Value::known(proposal_auth_calls_commit)),
            Witness::Base(Value::known(pallas::Base::from(self.proposal.creation_blockwindow))),
            Witness::Base(Value::known(pallas::Base::from(self.proposal.duration_blockwindows))),
            Witness::Base(Value::known(self.proposal.user_data)),
            Witness::Base(Value::known(self.proposal.blind.inner())),
            // DAO params
            Witness::Base(Value::known(dao_proposer_limit)),
            Witness::Base(Value::known(dao_quorum)),
            Witness::Base(Value::known(dao_early_exec_quorum)),
            Witness::Base(Value::known(dao_approval_ratio_quot)),
            Witness::Base(Value::known(dao_approval_ratio_base)),
            Witness::Base(Value::known(self.dao.gov_token_id.inner())),
            Witness::Base(Value::known(dao_notes_pub_x)),
            Witness::Base(Value::known(dao_notes_pub_y)),
            Witness::Base(Value::known(dao_proposer_pub_x)),
            Witness::Base(Value::known(dao_proposer_pub_y)),
            Witness::Base(Value::known(dao_proposals_pub_x)),
            Witness::Base(Value::known(dao_proposals_pub_y)),
            Witness::Base(Value::known(dao_votes_pub_x)),
            Witness::Base(Value::known(dao_votes_pub_y)),
            Witness::Base(Value::known(dao_exec_secret_key.inner())),
        ];
        // Early exec key
        match dao_early_exec_secret_key {
            Some(dao_early_exec_secret_key) => prover_witnesses
                .push(Witness::Base(Value::known(dao_early_exec_secret_key.inner()))),
            None => {
                let (dao_early_exec_pub_x, dao_early_exec_pub_y) =
                    self.dao.early_exec_public_key.xy();
                prover_witnesses.push(Witness::Base(Value::known(dao_early_exec_pub_x)));
                prover_witnesses.push(Witness::Base(Value::known(dao_early_exec_pub_y)));
            }
        };
        // Rest witnesses
        prover_witnesses.extend_from_slice(&[
            Witness::Base(Value::known(self.dao.bulla_blind.inner())),
            // Votes
            Witness::Base(Value::known(pallas::Base::from(self.yes_vote_value))),
            Witness::Base(Value::known(pallas::Base::from(self.all_vote_value))),
            Witness::Scalar(Value::known(self.yes_vote_blind.inner())),
            Witness::Scalar(Value::known(self.all_vote_blind.inner())),
            // Time checks
            Witness::Base(Value::known(current_blockwindow)),
            // Signature secret
            Witness::Base(Value::known(self.signature_secret.inner())),
        ]);

        debug!(target: "contract::dao::client::exec", "proposal_bulla: {proposal_bulla:?}");
        let public_inputs = vec![
            proposal_bulla.inner(),
            proposal_auth_calls_commit,
            current_blockwindow,
            *yes_vote_commit_coords.x(),
            *yes_vote_commit_coords.y(),
            *all_vote_commit_coords.x(),
            *all_vote_commit_coords.y(),
            signature_public.x(),
            signature_public.y(),
        ];
        //darkfi::zk::export_witness_json("proof/witness/exec.json", &prover_witnesses, &public_inputs);

        let circuit = ZkCircuit::new(prover_witnesses, exec_zkbin);
        let input_proof = Proof::create(exec_pk, &[circuit], &public_inputs, &mut OsRng)?;
        proofs.push(input_proof);

        let params = DaoExecParams {
            proposal_bulla,
            proposal_auth_calls: self.proposal.auth_calls,
            blind_total_vote: DaoBlindAggregateVote { yes_vote_commit, all_vote_commit },
            early_exec: dao_early_exec_secret_key.is_some(),
            signature_public,
        };

        Ok((params, proofs))
    }
}
