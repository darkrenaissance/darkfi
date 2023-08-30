/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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
    bridgetree,
    bridgetree::Hashable,
    crypto::{
        note::AeadEncryptedNote, pasta_prelude::*, pedersen::pedersen_commitment_u64,
        poseidon_hash, MerkleNode, PublicKey, SecretKey, TokenId,
    },
    pasta::pallas,
};
use darkfi_serial::{async_trait, SerialDecodable, SerialEncodable};
use rand::rngs::OsRng;

use darkfi::{
    zk::{halo2, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};

use crate::model::{DaoProposalBulla, DaoProposeParams, DaoProposeParamsInput};

use super::DaoInfo;

#[derive(SerialEncodable, SerialDecodable, Clone)]
pub struct DaoProposalInfo {
    pub dest: PublicKey,
    pub amount: u64,
    pub token_id: TokenId,
    pub blind: pallas::Base,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoProposeNote {
    pub proposal: DaoProposalInfo,
}

pub struct DaoProposeStakeInput {
    pub secret: SecretKey,
    pub note: darkfi_money_contract::client::MoneyNote,
    pub leaf_position: bridgetree::Position,
    pub merkle_path: Vec<MerkleNode>,
    pub signature_secret: SecretKey,
}

pub struct DaoProposeCall {
    pub inputs: Vec<DaoProposeStakeInput>,
    pub proposal: DaoProposalInfo,
    pub dao: DaoInfo,
    pub dao_leaf_position: bridgetree::Position,
    pub dao_merkle_path: Vec<MerkleNode>,
    pub dao_merkle_root: MerkleNode,
}

impl DaoProposeCall {
    pub fn make(
        self,
        burn_zkbin: &ZkBinary,
        burn_pk: &ProvingKey,
        main_zkbin: &ZkBinary,
        main_pk: &ProvingKey,
    ) -> Result<(DaoProposeParams, Vec<Proof>)> {
        let mut proofs = vec![];

        let gov_token_blind = pallas::Base::random(&mut OsRng);

        let mut inputs = vec![];
        let mut total_funds = 0;
        let mut total_funds_blinds = pallas::Scalar::from(0);

        for input in self.inputs {
            let funds_blind = pallas::Scalar::random(&mut OsRng);
            total_funds += input.note.value;
            total_funds_blinds += funds_blind;

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
                Witness::Scalar(halo2::Value::known(funds_blind)),
                Witness::Base(halo2::Value::known(gov_token_blind)),
                Witness::Uint32(halo2::Value::known(leaf_pos.try_into().unwrap())),
                Witness::MerklePath(halo2::Value::known(
                    input.merkle_path.clone().try_into().unwrap(),
                )),
                Witness::Base(halo2::Value::known(input.signature_secret.inner())),
            ];

            let public_key = PublicKey::from_secret(input.secret);
            let (pub_x, pub_y) = public_key.xy();

            let coin = poseidon_hash::<7>([
                pub_x,
                pub_y,
                pallas::Base::from(note.value),
                note.token_id.inner(),
                note.serial,
                pallas::Base::from(0),
                pallas::Base::from(0),
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

            let value_commit = pedersen_commitment_u64(note.value, funds_blind);
            let value_coords = value_commit.to_affine().coordinates().unwrap();

            let (sig_x, sig_y) = signature_public.xy();

            let public_inputs = vec![
                *value_coords.x(),
                *value_coords.y(),
                token_commit,
                merkle_root.inner(),
                sig_x,
                sig_y,
            ];
            let circuit = ZkCircuit::new(prover_witnesses, burn_zkbin);

            let proving_key = &burn_pk;
            let input_proof = Proof::create(proving_key, &[circuit], &public_inputs, &mut OsRng)
                .expect("DAO::propose() proving error!");
            proofs.push(input_proof);

            let input = DaoProposeParamsInput { value_commit, merkle_root, signature_public };
            inputs.push(input);
        }

        let total_funds_commit = pedersen_commitment_u64(total_funds, total_funds_blinds);
        let total_funds_coords = total_funds_commit.to_affine().coordinates().unwrap();
        let total_funds = pallas::Base::from(total_funds);

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

        let dao_leaf_position: u64 = self.dao_leaf_position.into();

        let proposal_bulla = DaoProposalBulla::from(poseidon_hash::<6>([
            proposal_dest_x,
            proposal_dest_y,
            proposal_amount,
            self.proposal.token_id.inner(),
            dao_bulla,
            self.proposal.blind,
        ]));

        let prover_witnesses = vec![
            // Proposers total number of gov tokens
            Witness::Base(halo2::Value::known(total_funds)),
            Witness::Scalar(halo2::Value::known(total_funds_blinds)),
            // Used for blinding exported gov token ID
            Witness::Base(halo2::Value::known(gov_token_blind)),
            // proposal params
            Witness::Base(halo2::Value::known(proposal_dest_x)),
            Witness::Base(halo2::Value::known(proposal_dest_y)),
            Witness::Base(halo2::Value::known(proposal_amount)),
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
            Witness::Uint32(halo2::Value::known(dao_leaf_position.try_into().unwrap())),
            Witness::MerklePath(halo2::Value::known(self.dao_merkle_path.try_into().unwrap())),
        ];
        let public_inputs = vec![
            token_commit,
            self.dao_merkle_root.inner(),
            proposal_bulla.inner(),
            *total_funds_coords.x(),
            *total_funds_coords.y(),
        ];
        let circuit = ZkCircuit::new(prover_witnesses, main_zkbin);

        let main_proof = Proof::create(main_pk, &[circuit], &public_inputs, &mut OsRng)
            .expect("DAO::propose() proving error!");
        proofs.push(main_proof);

        let note = DaoProposeNote { proposal: self.proposal };
        let enc_note = AeadEncryptedNote::encrypt(&note, &self.dao.public_key, &mut OsRng).unwrap();
        let params = DaoProposeParams {
            dao_merkle_root: self.dao_merkle_root,
            proposal_bulla,
            token_commit,
            note: enc_note,
            inputs,
        };

        Ok((params, proofs))
    }
}
