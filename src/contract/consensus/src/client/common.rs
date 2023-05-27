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

//! This API is crufty. Please rework it into something nice to read and nice to use.

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_money_contract::client::MoneyNote;
use darkfi_sdk::{
    crypto::{
        pasta_prelude::*, pedersen_commitment_base, pedersen_commitment_u64, poseidon_hash, Coin,
        MerkleNode, MerklePosition, Nullifier, PublicKey, SecretKey, TokenId,
    },
    incrementalmerkletree::Hashable,
    pasta::pallas,
};
use rand::rngs::OsRng;

use crate::client::ConsensusNote;

pub struct TransactionBuilderInputInfo {
    pub leaf_position: MerklePosition,
    pub merkle_path: Vec<MerkleNode>,
    pub secret: SecretKey,
    pub note: MoneyNote,
}

pub struct TransactionBuilderConsensusInputInfo {
    pub leaf_position: MerklePosition,
    pub merkle_path: Vec<MerkleNode>,
    pub secret: SecretKey,
    pub note: ConsensusNote,
}

pub struct TransactionBuilderOutputInfo {
    pub value: u64,
    pub token_id: TokenId,
    pub public_key: PublicKey,
}

pub struct ConsensusMintRevealed {
    pub epoch: u64,
    pub coin: Coin,
    pub value_commit: pallas::Point,
}

impl ConsensusMintRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();
        let epoch_palas = pallas::Base::from(self.epoch);

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![epoch_palas, self.coin.inner(), *valcom_coords.x(), *valcom_coords.y()]
    }
}

pub fn create_consensus_mint_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    epoch: u64,
    output: &TransactionBuilderOutputInfo,
    value_blind: pallas::Scalar,
    serial: pallas::Base,
    coin_blind: pallas::Base,
) -> Result<(Proof, ConsensusMintRevealed)> {
    let epoch_pallas = pallas::Base::from(epoch);
    let value_pallas = pallas::Base::from(output.value);
    let value_commit = pedersen_commitment_u64(output.value, value_blind);
    let (pub_x, pub_y) = output.public_key.xy();

    let coin =
        Coin::from(poseidon_hash([pub_x, pub_y, value_pallas, epoch_pallas, serial, coin_blind]));

    let public_inputs = ConsensusMintRevealed { epoch, coin, value_commit };

    let prover_witnesses = vec![
        Witness::Base(Value::known(pub_x)),
        Witness::Base(Value::known(pub_y)),
        Witness::Base(Value::known(value_pallas)),
        Witness::Base(Value::known(epoch_pallas)),
        Witness::Base(Value::known(serial)),
        Witness::Base(Value::known(coin_blind)),
        Witness::Scalar(Value::known(value_blind)),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}

pub struct ConsensusBurnRevealed {
    pub nullifier: Nullifier,
    pub epoch: u64,
    pub signature_public: PublicKey,
    pub merkle_root: MerkleNode,
    pub value_commit: pallas::Point,
}

impl ConsensusBurnRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();
        let sigpub_coords = self.signature_public.inner().to_affine().coordinates().unwrap();
        let epoch_palas = pallas::Base::from(self.epoch);

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![
            self.nullifier.inner(),
            epoch_palas,
            *sigpub_coords.x(),
            *sigpub_coords.y(),
            self.merkle_root.inner(),
            *valcom_coords.x(),
            *valcom_coords.y(),
        ]
    }
}

pub fn create_consensus_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &TransactionBuilderConsensusInputInfo,
    value_blind: pallas::Scalar,
) -> Result<(Proof, ConsensusBurnRevealed, SecretKey)> {
    let nullifier = Nullifier::from(poseidon_hash([input.secret.inner(), input.note.serial]));
    let epoch = input.note.epoch;
    let epoch_pallas = pallas::Base::from(epoch);
    let value_pallas = pallas::Base::from(input.note.value);
    let value_commit = pedersen_commitment_u64(input.note.value, value_blind);
    let public_key = PublicKey::from_secret(input.secret);
    let (pub_x, pub_y) = public_key.xy();

    let coin = poseidon_hash([
        pub_x,
        pub_y,
        value_pallas,
        epoch_pallas,
        input.note.serial,
        input.note.coin_blind,
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

    let public_inputs = ConsensusBurnRevealed {
        nullifier,
        epoch,
        signature_public: public_key,
        merkle_root,
        value_commit,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(value_pallas)),
        Witness::Base(Value::known(epoch_pallas)),
        Witness::Base(Value::known(input.note.serial)),
        Witness::Base(Value::known(input.note.coin_blind)),
        Witness::Scalar(Value::known(value_blind)),
        Witness::Base(Value::known(input.secret.inner())),
        Witness::Uint32(Value::known(u64::from(input.leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(input.merkle_path.clone().try_into().unwrap())),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs, input.secret))
}

// TODO: Remove everything following
pub struct ConsensusUnstakeBurnRevealed {
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Point,
    pub nullifier: Nullifier,
    pub merkle_root: MerkleNode,
    pub spend_hook: pallas::Base,
    pub user_data_enc: pallas::Base,
    pub signature_public: PublicKey,
}

impl ConsensusUnstakeBurnRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();
        let tokcom_coords = self.token_commit.to_affine().coordinates().unwrap();
        let sigpub_coords = self.signature_public.inner().to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![
            self.nullifier.inner(),
            *valcom_coords.x(),
            *valcom_coords.y(),
            *tokcom_coords.x(),
            *tokcom_coords.y(),
            self.merkle_root.inner(),
            self.user_data_enc,
            *sigpub_coords.x(),
            *sigpub_coords.y(),
        ]
    }
}

pub fn create_unstake_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &TransactionBuilderInputInfo,
    value_blind: pallas::Scalar,
    token_blind: pallas::Scalar,
    user_data_blind: pallas::Base,
    signature_secret: SecretKey,
) -> Result<(Proof, ConsensusUnstakeBurnRevealed)> {
    let nullifier = Nullifier::from(poseidon_hash([input.secret.inner(), input.note.serial]));
    let public_key = PublicKey::from_secret(input.secret);
    let (pub_x, pub_y) = public_key.xy();

    let signature_public = PublicKey::from_secret(signature_secret);

    let coin = poseidon_hash([
        pub_x,
        pub_y,
        pallas::Base::from(input.note.value),
        input.note.token_id.inner(),
        input.note.serial,
        input.note.spend_hook,
        input.note.user_data,
        input.note.coin_blind,
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

    let user_data_enc = poseidon_hash([input.note.user_data, user_data_blind]);
    let value_commit = pedersen_commitment_u64(input.note.value, value_blind);
    let token_commit = pedersen_commitment_base(input.note.token_id.inner(), token_blind);

    let public_inputs = ConsensusUnstakeBurnRevealed {
        value_commit,
        token_commit,
        nullifier,
        merkle_root,
        spend_hook: input.note.spend_hook,
        user_data_enc,
        signature_public,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(pallas::Base::from(input.note.value))),
        Witness::Base(Value::known(input.note.token_id.inner())),
        Witness::Scalar(Value::known(value_blind)),
        Witness::Scalar(Value::known(token_blind)),
        Witness::Base(Value::known(input.note.serial)),
        Witness::Base(Value::known(input.note.spend_hook)),
        Witness::Base(Value::known(input.note.user_data)),
        Witness::Base(Value::known(user_data_blind)),
        Witness::Base(Value::known(input.note.coin_blind)),
        Witness::Base(Value::known(input.secret.inner())),
        Witness::Uint32(Value::known(u64::from(input.leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(input.merkle_path.clone().try_into().unwrap())),
        Witness::Base(Value::known(signature_secret.inner())),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
