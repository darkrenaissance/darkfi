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
        note::AeadEncryptedNote,
        pasta_prelude::*,
        pedersen::pedersen_commitment_u64,
        poseidon_hash,
        smt::{PoseidonFp, SparseMerkleTree, StorageAdapter, SMT_FP_DEPTH},
        Blind, FuncId, MerkleNode, PublicKey, ScalarBlind, SecretKey,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    ClientFailed, Result,
};

use crate::{
    error::DaoError,
    model::{Dao, DaoProposal, DaoProposeParams, DaoProposeParamsInput, VecAuthCallCommit},
};

pub struct DaoProposeStakeInput {
    pub secret: SecretKey,
    pub note: darkfi_money_contract::client::MoneyNote,
    pub leaf_position: bridgetree::Position,
    pub merkle_path: Vec<MerkleNode>,
}

pub struct DaoProposeCall<'a, T: StorageAdapter<Value = pallas::Base>> {
    pub money_null_smt:
        &'a SparseMerkleTree<'a, SMT_FP_DEPTH, { SMT_FP_DEPTH + 1 }, pallas::Base, PoseidonFp, T>,
    pub inputs: Vec<DaoProposeStakeInput>,
    pub proposal: DaoProposal,
    pub dao: Dao,
    pub dao_leaf_position: bridgetree::Position,
    pub dao_merkle_path: Vec<MerkleNode>,
    pub dao_merkle_root: MerkleNode,
}

impl<T: StorageAdapter<Value = pallas::Base>> DaoProposeCall<'_, T> {
    pub fn make(
        self,
        dao_proposer_secret_key: &SecretKey,
        burn_zkbin: &ZkBinary,
        burn_pk: &ProvingKey,
        main_zkbin: &ZkBinary,
        main_pk: &ProvingKey,
    ) -> Result<(DaoProposeParams, Vec<Proof>, Vec<SecretKey>)> {
        let mut proofs = vec![];
        let mut signature_secrets = vec![];

        let gov_token_blind = Blind::random(&mut OsRng);
        let smt_null_root = self.money_null_smt.root();

        let mut inputs = vec![];
        let mut total_funds = 0;
        let mut total_funds_blinds = ScalarBlind::ZERO;

        for input in self.inputs {
            let funds_blind = Blind::random(&mut OsRng);
            total_funds += input.note.value;
            total_funds_blinds += funds_blind;

            // Note from the previous output
            let note = input.note;
            let leaf_pos: u64 = input.leaf_position.into();

            let public_key = PublicKey::from_secret(input.secret);
            let coin = CoinAttributes {
                public_key,
                value: note.value,
                token_id: note.token_id,
                spend_hook: FuncId::none(),
                user_data: pallas::Base::ZERO,
                blind: note.coin_blind,
            }
            .to_coin();
            let nullifier = poseidon_hash([input.secret.inner(), coin.inner()]);

            let smt_null_path = self.money_null_smt.prove_membership(&nullifier);
            if !smt_null_path.verify(&smt_null_root, &pallas::Base::ZERO, &nullifier) {
                return Err(
                    ClientFailed::VerifyError(DaoError::InvalidInputMerkleRoot.to_string()).into()
                )
            }

            let signature_secret = SecretKey::random(&mut OsRng);
            let signature_public = PublicKey::from_secret(signature_secret);
            let (sig_x, sig_y) = signature_public.xy();

            let prover_witnesses = vec![
                Witness::Base(Value::known(input.secret.inner())),
                Witness::Base(Value::known(pallas::Base::from(note.value))),
                Witness::Base(Value::known(note.token_id.inner())),
                Witness::Base(Value::known(pallas::Base::ZERO)),
                Witness::Base(Value::known(pallas::Base::ZERO)),
                Witness::Base(Value::known(note.coin_blind.inner())),
                Witness::Scalar(Value::known(funds_blind.inner())),
                Witness::Base(Value::known(gov_token_blind.inner())),
                Witness::Uint32(Value::known(leaf_pos.try_into().unwrap())),
                Witness::MerklePath(Value::known(input.merkle_path.clone().try_into().unwrap())),
                Witness::SparseMerklePath(Value::known(smt_null_path.path)),
                Witness::Base(Value::known(signature_secret.inner())),
            ];

            // TODO: We need a generic ZkSet widget to avoid doing this all the time

            let merkle_coin_root = {
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

            let token_commit = poseidon_hash([note.token_id.inner(), gov_token_blind.inner()]);
            if note.token_id != self.dao.gov_token_id {
                return Err(ClientFailed::InvalidTokenId(note.token_id.to_string()).into())
            }

            let value_commit = pedersen_commitment_u64(note.value, funds_blind);
            let value_coords = value_commit.to_affine().coordinates().unwrap();

            let public_inputs = vec![
                smt_null_root,
                *value_coords.x(),
                *value_coords.y(),
                token_commit,
                merkle_coin_root.inner(),
                sig_x,
                sig_y,
            ];
            //darkfi::zk::export_witness_json("proof/witness/propose-input.json", &prover_witnesses, &public_inputs);
            let circuit = ZkCircuit::new(prover_witnesses, burn_zkbin);

            let proving_key = &burn_pk;
            let input_proof = Proof::create(proving_key, &[circuit], &public_inputs, &mut OsRng)?;
            proofs.push(input_proof);
            signature_secrets.push(signature_secret);

            let input = DaoProposeParamsInput {
                value_commit,
                merkle_coin_root,
                smt_null_root,
                signature_public,
            };
            inputs.push(input);
        }

        let total_funds_commit = pedersen_commitment_u64(total_funds, total_funds_blinds);
        let total_funds_coords = total_funds_commit.to_affine().coordinates().unwrap();
        let total_funds = pallas::Base::from(total_funds);

        let token_commit = poseidon_hash([self.dao.gov_token_id.inner(), gov_token_blind.inner()]);

        let dao_proposer_limit = pallas::Base::from(self.dao.proposer_limit);
        let dao_quorum = pallas::Base::from(self.dao.quorum);
        let dao_early_exec_quorum = pallas::Base::from(self.dao.early_exec_quorum);
        let dao_approval_ratio_quot = pallas::Base::from(self.dao.approval_ratio_quot);
        let dao_approval_ratio_base = pallas::Base::from(self.dao.approval_ratio_base);
        let (dao_notes_pub_x, dao_notes_pub_y) = self.dao.notes_public_key.xy();
        let (dao_proposals_pub_x, dao_proposals_pub_y) = self.dao.proposals_public_key.xy();
        let (dao_votes_pub_x, dao_votes_pub_y) = self.dao.votes_public_key.xy();
        let (dao_exec_pub_x, dao_exec_pub_y) = self.dao.exec_public_key.xy();
        let (dao_early_exec_pub_x, dao_early_exec_pub_y) = self.dao.early_exec_public_key.xy();

        let dao_leaf_position: u64 = self.dao_leaf_position.into();

        if self.dao.to_bulla() != self.proposal.dao_bulla {
            return Err(ClientFailed::VerifyError(DaoError::InvalidCalls.to_string()).into())
        }
        let proposal_bulla = self.proposal.to_bulla();

        let prover_witnesses = vec![
            // Proposers total number of gov tokens
            Witness::Base(Value::known(total_funds)),
            Witness::Scalar(Value::known(total_funds_blinds.inner())),
            // Used for blinding exported gov token ID
            Witness::Base(Value::known(gov_token_blind.inner())),
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
            Witness::Base(Value::known(dao_proposer_secret_key.inner())),
            Witness::Base(Value::known(dao_proposals_pub_x)),
            Witness::Base(Value::known(dao_proposals_pub_y)),
            Witness::Base(Value::known(dao_votes_pub_x)),
            Witness::Base(Value::known(dao_votes_pub_y)),
            Witness::Base(Value::known(dao_exec_pub_x)),
            Witness::Base(Value::known(dao_exec_pub_y)),
            Witness::Base(Value::known(dao_early_exec_pub_x)),
            Witness::Base(Value::known(dao_early_exec_pub_y)),
            Witness::Base(Value::known(self.dao.bulla_blind.inner())),
            Witness::Uint32(Value::known(dao_leaf_position.try_into().unwrap())),
            Witness::MerklePath(Value::known(self.dao_merkle_path.try_into().unwrap())),
        ];
        let public_inputs = vec![
            token_commit,
            self.dao_merkle_root.inner(),
            proposal_bulla.inner(),
            pallas::Base::from(self.proposal.creation_blockwindow),
            *total_funds_coords.x(),
            *total_funds_coords.y(),
        ];
        //darkfi::zk::export_witness_json("proof/witness/propose-main.json", &prover_witnesses, &public_inputs);
        let circuit = ZkCircuit::new(prover_witnesses, main_zkbin);

        let main_proof = Proof::create(main_pk, &[circuit], &public_inputs, &mut OsRng)?;
        proofs.push(main_proof);

        let enc_note =
            AeadEncryptedNote::encrypt(&self.proposal, &self.dao.proposals_public_key, &mut OsRng)
                .unwrap();
        let params = DaoProposeParams {
            dao_merkle_root: self.dao_merkle_root,
            proposal_bulla,
            token_commit,
            note: enc_note,
            inputs,
        };

        Ok((params, proofs, signature_secrets))
    }
}
