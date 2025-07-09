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

use darkfi_money_contract::model::CoinAttributes;
use darkfi_sdk::{
    bridgetree,
    bridgetree::Hashable,
    crypto::{
        note::ElGamalEncryptedNote,
        pasta_prelude::*,
        pedersen_commitment_u64, poseidon_hash,
        smt::{PoseidonFp, SparseMerkleTree, StorageAdapter, SMT_FP_DEPTH},
        util::fv_mod_fp_unsafe,
        Blind, MerkleNode, PublicKey, SecretKey,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    ClientFailed, Result,
};

use crate::{
    error::DaoError,
    model::{Dao, DaoProposal, DaoVoteParams, DaoVoteParamsInput, VecAuthCallCommit},
};

pub struct DaoVoteInput {
    pub secret: SecretKey,
    pub note: darkfi_money_contract::client::MoneyNote,
    pub leaf_position: bridgetree::Position,
    pub merkle_path: Vec<MerkleNode>,
    pub signature_secret: SecretKey,
}

// Inside ZK proof, check proposal is correct.
pub struct DaoVoteCall<'a, T: StorageAdapter<Value = pallas::Base>> {
    pub money_null_smt:
        &'a SparseMerkleTree<'a, SMT_FP_DEPTH, { SMT_FP_DEPTH + 1 }, pallas::Base, PoseidonFp, T>,
    pub inputs: Vec<DaoVoteInput>,
    pub vote_option: bool,
    pub proposal: DaoProposal,
    pub dao: Dao,
    pub current_blockwindow: u64,
}

impl<T: StorageAdapter<Value = pallas::Base>> DaoVoteCall<'_, T> {
    pub fn make(
        self,
        burn_zkbin: &ZkBinary,
        burn_pk: &ProvingKey,
        main_zkbin: &ZkBinary,
        main_pk: &ProvingKey,
    ) -> Result<(DaoVoteParams, Vec<Proof>)> {
        debug!(target: "contract::dao::client::vote", "make()");

        if self.dao.to_bulla() != self.proposal.dao_bulla {
            return Err(ClientFailed::VerifyError(DaoError::InvalidCalls.to_string()).into())
        }
        let proposal_bulla = self.proposal.to_bulla();

        let mut proofs = vec![];

        let gov_token_blind = pallas::Base::random(&mut OsRng);

        let mut inputs = vec![];
        let mut all_vote_value = 0;
        let mut all_vote_blind = pallas::Scalar::from(0);

        let last_input_idx = self.inputs.len() - 1;
        for (i, input) in self.inputs.into_iter().enumerate() {
            // Last input
            // Choose a blinding factor that can be converted to pallas::Base exactly.
            // We need this so we can verifiably encrypt the sum of input blinds
            // in the next section.
            // TODO: make a generalized widget for this, and also picking blinds in money::transfer()
            let mut value_blind = pallas::Scalar::random(&mut OsRng);

            if i == last_input_idx {
                // It's near zero chance it ever loops at all.
                // P(random 𝔽ᵥ ∉ 𝔽ₚ) = (q - p)/q = 2.99 × 10⁻⁵¹
                loop {
                    let av_blind = fv_mod_fp_unsafe(all_vote_blind + value_blind);

                    if av_blind.is_none().into() {
                        value_blind = pallas::Scalar::random(&mut OsRng);
                        continue
                    }

                    break
                }
            }

            all_vote_value += input.note.value;
            all_vote_blind += value_blind;

            let signature_public = PublicKey::from_secret(input.signature_secret);

            // Note from the previous output
            let note = input.note;
            let leaf_pos: u64 = input.leaf_position.into();

            let public_key = PublicKey::from_secret(input.secret);
            let coin = CoinAttributes {
                public_key,
                value: note.value,
                token_id: note.token_id,
                spend_hook: note.spend_hook,
                user_data: note.user_data,
                blind: note.coin_blind,
            }
            .to_coin();
            let nullifier = poseidon_hash([input.secret.inner(), coin.inner()]);

            let smt_null_root = self.money_null_smt.root();
            let smt_null_path = self.money_null_smt.prove_membership(&nullifier);
            if !smt_null_path.verify(&smt_null_root, &pallas::Base::ZERO, &nullifier) {
                return Err(
                    ClientFailed::VerifyError(DaoError::InvalidInputMerkleRoot.to_string()).into()
                )
            }

            let prover_witnesses = vec![
                Witness::Base(Value::known(input.secret.inner())),
                Witness::Base(Value::known(pallas::Base::from(note.value))),
                Witness::Base(Value::known(note.token_id.inner())),
                Witness::Base(Value::known(pallas::Base::ZERO)),
                Witness::Base(Value::known(pallas::Base::ZERO)),
                Witness::Base(Value::known(note.coin_blind.inner())),
                Witness::Base(Value::known(proposal_bulla.inner())),
                Witness::Scalar(Value::known(value_blind)),
                Witness::Base(Value::known(gov_token_blind)),
                Witness::Uint32(Value::known(leaf_pos.try_into().unwrap())),
                Witness::MerklePath(Value::known(input.merkle_path.clone().try_into().unwrap())),
                Witness::SparseMerklePath(Value::known(smt_null_path.path)),
                Witness::Base(Value::known(input.signature_secret.inner())),
            ];

            let merkle_root = {
                let position: u64 = input.leaf_position.into();
                let mut current = MerkleNode::from(coin.inner());
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

            let token_commit = poseidon_hash([note.token_id.inner(), gov_token_blind]);
            if note.token_id != self.dao.gov_token_id {
                return Err(ClientFailed::InvalidTokenId(note.token_id.to_string()).into())
            }

            let vote_commit = pedersen_commitment_u64(note.value, Blind(value_blind));
            let vote_commit_coords = vote_commit.to_affine().coordinates().unwrap();

            let (sig_x, sig_y) = signature_public.xy();

            let vote_nullifier =
                poseidon_hash([nullifier, input.secret.inner(), proposal_bulla.inner()]);

            let public_inputs = vec![
                smt_null_root,
                proposal_bulla.inner(),
                vote_nullifier,
                *vote_commit_coords.x(),
                *vote_commit_coords.y(),
                token_commit,
                merkle_root.inner(),
                sig_x,
                sig_y,
            ];

            //darkfi::zk::export_witness_json("proof/witness/vote-input.json", &prover_witnesses, &public_inputs);
            let circuit = ZkCircuit::new(prover_witnesses, burn_zkbin);
            debug!(target: "contract::dao::client::vote", "input_proof Proof::create()");
            let input_proof = Proof::create(burn_pk, &[circuit], &public_inputs, &mut OsRng)?;
            proofs.push(input_proof);

            let input = DaoVoteParamsInput {
                vote_commit,
                vote_nullifier: vote_nullifier.into(),
                signature_public,
            };
            inputs.push(input);
        }

        let token_commit = poseidon_hash([self.dao.gov_token_id.inner(), gov_token_blind]);

        let dao_proposer_limit = pallas::Base::from(self.dao.proposer_limit);
        let dao_quorum = pallas::Base::from(self.dao.quorum);
        let dao_early_exec_quorum = pallas::Base::from(self.dao.early_exec_quorum);
        let dao_approval_ratio_quot = pallas::Base::from(self.dao.approval_ratio_quot);
        let dao_approval_ratio_base = pallas::Base::from(self.dao.approval_ratio_base);
        let (dao_notes_pub_x, dao_notes_pub_y) = self.dao.notes_public_key.xy();
        let (dao_proposer_pub_x, dao_proposer_pub_y) = self.dao.proposer_public_key.xy();
        let (dao_proposals_pub_x, dao_proposals_pub_y) = self.dao.proposals_public_key.xy();
        let dao_votes_public_key = self.dao.votes_public_key.inner();
        let (dao_exec_pub_x, dao_exec_pub_y) = self.dao.exec_public_key.xy();
        let (dao_early_exec_pub_x, dao_early_exec_pub_y) = self.dao.early_exec_public_key.xy();

        let vote_option = self.vote_option as u64;
        if vote_option != 0 && vote_option != 1 {
            return Err(ClientFailed::VerifyError(DaoError::VoteInputsEmpty.to_string()).into())
        }

        // Create a random blind b ∈ 𝔽ᵥ, such that b ∈ 𝔽ₚ
        let yes_vote_blind = loop {
            let blind = pallas::Scalar::random(&mut OsRng);
            if fv_mod_fp_unsafe(blind).is_some().into() {
                break blind
            }
        };
        let yes_vote_commit =
            pedersen_commitment_u64(vote_option * all_vote_value, Blind(yes_vote_blind));
        let yes_vote_commit_coords = yes_vote_commit.to_affine().coordinates().unwrap();

        let all_vote_commit = pedersen_commitment_u64(all_vote_value, Blind(all_vote_blind));
        if all_vote_commit != inputs.iter().map(|i| i.vote_commit).sum() {
            return Err(ClientFailed::VerifyError(DaoError::VoteCommitMismatch.to_string()).into())
        }
        let all_vote_commit_coords = all_vote_commit.to_affine().coordinates().unwrap();

        // Convert blinds to 𝔽ₚ, which should work fine since we selected them
        // to be convertable.
        let yes_vote_blind = Blind(fv_mod_fp_unsafe(yes_vote_blind).unwrap());
        let all_vote_blind = Blind(fv_mod_fp_unsafe(all_vote_blind).unwrap());

        let vote_option = pallas::Base::from(vote_option);
        let all_vote_value_fp = pallas::Base::from(all_vote_value);
        let ephem_secret = SecretKey::random(&mut OsRng);
        let ephem_pubkey = PublicKey::from_secret(ephem_secret);
        let (ephem_x, ephem_y) = ephem_pubkey.xy();

        let current_blockwindow = pallas::Base::from(self.current_blockwindow);

        let prover_witnesses = vec![
            // Proposal params
            Witness::Base(Value::known(self.proposal.auth_calls.commit())),
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
            Witness::EcNiPoint(Value::known(dao_votes_public_key)),
            Witness::Base(Value::known(dao_exec_pub_x)),
            Witness::Base(Value::known(dao_exec_pub_y)),
            Witness::Base(Value::known(dao_early_exec_pub_x)),
            Witness::Base(Value::known(dao_early_exec_pub_y)),
            Witness::Base(Value::known(self.dao.bulla_blind.inner())),
            // Vote
            Witness::Base(Value::known(vote_option)),
            Witness::Base(Value::known(yes_vote_blind.inner())),
            // Total number of gov tokens allocated
            Witness::Base(Value::known(all_vote_value_fp)),
            Witness::Base(Value::known(all_vote_blind.inner())),
            // Gov token
            Witness::Base(Value::known(gov_token_blind)),
            // Time checks
            Witness::Base(Value::known(current_blockwindow)),
            // verifiable encryption
            Witness::Base(Value::known(ephem_secret.inner())),
        ];

        let note = [vote_option, yes_vote_blind.inner(), all_vote_value_fp, all_vote_blind.inner()];
        let enc_note =
            ElGamalEncryptedNote::encrypt_unsafe(note, &ephem_secret, &self.dao.votes_public_key)?;

        let public_inputs = vec![
            token_commit,
            proposal_bulla.inner(),
            *yes_vote_commit_coords.x(),
            *yes_vote_commit_coords.y(),
            *all_vote_commit_coords.x(),
            *all_vote_commit_coords.y(),
            current_blockwindow,
            ephem_x,
            ephem_y,
            enc_note.encrypted_values[0],
            enc_note.encrypted_values[1],
            enc_note.encrypted_values[2],
            enc_note.encrypted_values[3],
        ];

        //darkfi::zk::export_witness_json("proof/witness/vote-main.json", &prover_witnesses, &public_inputs);
        let circuit = ZkCircuit::new(prover_witnesses, main_zkbin);

        debug!(target: "contract::dao::client::vote", "main_proof = Proof::create()");
        let main_proof = Proof::create(main_pk, &[circuit], &public_inputs, &mut OsRng)?;
        proofs.push(main_proof);

        let params =
            DaoVoteParams { token_commit, proposal_bulla, yes_vote_commit, note: enc_note, inputs };

        Ok((params, proofs))
    }
}
