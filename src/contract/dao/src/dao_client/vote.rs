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

use darkfi_sdk::crypto::{
    merkle_prelude::*, pallas, pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, Keypair,
    MerkleNode, MerklePosition, Nullifier, PublicKey, SecretKey,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};
use log::debug;
use rand::rngs::OsRng;

use darkfi::{
    zk::{halo2, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};

use super::{DaoInfo, ProposalInfo};
use crate::{
    dao_model::{VoteCallParams, VoteCallParamsInput},
    note,
};

#[derive(SerialEncodable, SerialDecodable)]
pub struct VoteNote {
    pub vote: VoteInfo,
    pub vote_value: u64,
    pub vote_value_blind: pallas::Scalar,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct VoteInfo {
    pub vote_option: bool,
    pub vote_option_blind: pallas::Scalar,
}

pub struct VoteInput {
    pub secret: SecretKey,
    //pub note: money::transfer::wallet::Note,
    pub note: darkfi_money_contract::client::Note,
    pub leaf_position: MerklePosition,
    pub merkle_path: Vec<MerkleNode>,
    pub signature_secret: SecretKey,
}

// TODO: should be token locking voting?
// Inside ZKproof, check proposal is correct.
pub struct VoteCall {
    pub inputs: Vec<VoteInput>,
    pub vote: VoteInfo,
    pub vote_keypair: Keypair,
    pub proposal: ProposalInfo,
    pub dao: DaoInfo,
}

impl VoteCall {
    pub fn make(
        self,
        burn_zkbin: &ZkBinary,
        burn_pk: &ProvingKey,
        main_zkbin: &ZkBinary,
        main_pk: &ProvingKey,
    ) -> Result<(VoteCallParams, Vec<Proof>)> {
        debug!(target: "dao", "build()");
        let mut proofs = vec![];

        let gov_token_blind = pallas::Base::random(&mut OsRng);

        let mut inputs = vec![];
        let mut vote_value = 0;
        let mut vote_value_blind = pallas::Scalar::from(0);

        for input in self.inputs {
            let value_blind = pallas::Scalar::random(&mut OsRng);

            vote_value += input.note.value;
            vote_value_blind += value_blind;

            let signature_public = PublicKey::from_secret(input.signature_secret);

            // Note from the previous output
            let note = input.note;
            let leaf_pos: u64 = input.leaf_position.into();

            let prover_witnesses = vec![
                Witness::Base(halo2::Value::known(input.secret.inner())),
                Witness::Base(halo2::Value::known(note.serial)),
                Witness::Base(halo2::Value::known(pallas::Base::from(0))),
                Witness::Base(halo2::Value::known(pallas::Base::from(0))),
                Witness::Base(halo2::Value::known(pallas::Base::from(note.value))),
                Witness::Base(halo2::Value::known(note.token_id.inner())),
                Witness::Base(halo2::Value::known(note.coin_blind)),
                Witness::Scalar(halo2::Value::known(vote_value_blind)),
                Witness::Base(halo2::Value::known(gov_token_blind)),
                Witness::Uint32(halo2::Value::known(leaf_pos.try_into().unwrap())),
                Witness::MerklePath(halo2::Value::known(
                    input.merkle_path.clone().try_into().unwrap(),
                )),
                Witness::Base(halo2::Value::known(input.signature_secret.inner())),
            ];

            let public_key = PublicKey::from_secret(input.secret);
            let (pub_x, pub_y) = public_key.xy();

            let coin = poseidon_hash::<8>([
                pub_x,
                pub_y,
                pallas::Base::from(note.value),
                note.token_id.inner(),
                note.serial,
                pallas::Base::from(0),
                pallas::Base::from(0),
                note.coin_blind,
            ]);

            let merkle_root = {
                let position: u64 = input.leaf_position.into();
                let mut current = MerkleNode::from(coin);
                for (level, sibling) in input.merkle_path.iter().enumerate() {
                    let level = level as u8;
                    current = if position & (1 << level) == 0 {
                        MerkleNode::combine(level.into(), &current, sibling)
                    } else {
                        MerkleNode::combine(level.into(), sibling, &current)
                    };
                }
                current
            };

            let token_commit = poseidon_hash::<2>([note.token_id.inner(), gov_token_blind]);
            assert_eq!(self.dao.gov_token_id, note.token_id);

            let nullifier = poseidon_hash::<2>([input.secret.inner(), note.serial]);

            let vote_commit = pedersen_commitment_u64(note.value, vote_value_blind);
            let vote_commit_coords = vote_commit.to_affine().coordinates().unwrap();

            let (sig_x, sig_y) = signature_public.xy();

            let public_inputs = vec![
                nullifier,
                *vote_commit_coords.x(),
                *vote_commit_coords.y(),
                token_commit,
                merkle_root.inner(),
                sig_x,
                sig_y,
            ];

            let circuit = ZkCircuit::new(prover_witnesses, burn_zkbin.clone());
            debug!(target: "dao", "input_proof Proof::create()");
            let input_proof = Proof::create(&burn_pk, &[circuit], &public_inputs, &mut OsRng)
                .expect("DAO::vote() proving error!");
            proofs.push(input_proof);

            let input = VoteCallParamsInput {
                nullifier: Nullifier::from(nullifier),
                vote_commit,
                merkle_root,
                signature_public,
            };
            inputs.push(input);
        }

        let token_commit = poseidon_hash::<2>([self.dao.gov_token_id.inner(), gov_token_blind]);

        let (proposal_dest_x, proposal_dest_y) = self.proposal.dest.xy();

        let proposal_amount = pallas::Base::from(self.proposal.amount);

        let dao_proposer_limit = pallas::Base::from(self.dao.proposer_limit);
        let dao_quorum = pallas::Base::from(self.dao.quorum);
        let dao_approval_ratio_quot = pallas::Base::from(self.dao.approval_ratio_quot);
        let dao_approval_ratio_base = pallas::Base::from(self.dao.approval_ratio_base);

        let (dao_pub_x, dao_pub_y) = self.dao.public_key.xy();

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

        let vote_option = self.vote.vote_option as u64;
        assert!(vote_option == 0 || vote_option == 1);

        let yes_vote_commit =
            pedersen_commitment_u64(vote_option * vote_value, self.vote.vote_option_blind);
        let yes_vote_commit_coords = yes_vote_commit.to_affine().coordinates().unwrap();

        let all_vote_commit = pedersen_commitment_u64(vote_value, vote_value_blind);
        let all_vote_commit_coords = all_vote_commit.to_affine().coordinates().unwrap();

        let prover_witnesses = vec![
            // proposal params
            Witness::Base(halo2::Value::known(proposal_dest_x)),
            Witness::Base(halo2::Value::known(proposal_dest_y)),
            Witness::Base(halo2::Value::known(proposal_amount)),
            Witness::Base(halo2::Value::known(self.proposal.serial)),
            Witness::Base(halo2::Value::known(self.proposal.token_id.inner())),
            Witness::Base(halo2::Value::known(self.proposal.blind)),
            // DAO params
            Witness::Base(halo2::Value::known(dao_proposer_limit)),
            Witness::Base(halo2::Value::known(dao_quorum)),
            Witness::Base(halo2::Value::known(dao_approval_ratio_quot)),
            Witness::Base(halo2::Value::known(dao_approval_ratio_base)),
            Witness::Base(halo2::Value::known(self.dao.gov_token_id.inner())),
            Witness::Base(halo2::Value::known(dao_pub_x)),
            Witness::Base(halo2::Value::known(dao_pub_y)),
            Witness::Base(halo2::Value::known(self.dao.bulla_blind)),
            // Vote
            Witness::Base(halo2::Value::known(pallas::Base::from(vote_option))),
            Witness::Scalar(halo2::Value::known(self.vote.vote_option_blind)),
            // Total number of gov tokens allocated
            Witness::Base(halo2::Value::known(pallas::Base::from(vote_value))),
            Witness::Scalar(halo2::Value::known(vote_value_blind)),
            // gov token
            Witness::Base(halo2::Value::known(gov_token_blind)),
        ];

        let public_inputs = vec![
            token_commit,
            proposal_bulla,
            // this should be a value commit??
            *yes_vote_commit_coords.x(),
            *yes_vote_commit_coords.y(),
            *all_vote_commit_coords.x(),
            *all_vote_commit_coords.y(),
        ];

        let circuit = ZkCircuit::new(prover_witnesses, main_zkbin.clone());

        debug!(target: "dao", "main_proof = Proof::create()");
        let main_proof = Proof::create(&main_pk, &[circuit], &public_inputs, &mut OsRng)
            .expect("DAO::vote() proving error!");
        proofs.push(main_proof);

        let note = VoteNote { vote: self.vote, vote_value, vote_value_blind };
        let enc_note = note::encrypt(&note, &self.vote_keypair.public).unwrap();

        let params = VoteCallParams {
            token_commit,
            proposal_bulla,
            yes_vote_commit,

            ciphertext: enc_note.ciphertext,
            ephem_public: enc_note.ephem_public,
            inputs,
        };

        Ok((params, proofs))
    }
}
