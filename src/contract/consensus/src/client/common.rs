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
use darkfi_money_contract::{client::ConsensusNote, model::Coin};
use darkfi_sdk::{
    crypto::{
        pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, MerkleNode, MerklePosition,
        Nullifier, PublicKey, SecretKey,
    },
    incrementalmerkletree::Hashable,
    pasta::pallas,
};
use rand::rngs::OsRng;

pub struct ConsensusMintOutputInfo {
    pub value: u64,
    pub epoch: u64,
    pub public_key: PublicKey,
    pub value_blind: pallas::Scalar,
    pub serial: pallas::Base,
    pub coin_blind: pallas::Base,
}

pub struct ConsensusBurnInputInfo {
    pub leaf_position: MerklePosition,
    pub merkle_path: Vec<MerkleNode>,
    pub secret: SecretKey,
    pub note: ConsensusNote,
    pub value_blind: pallas::Scalar,
}

pub struct ConsensusMintRevealed {
    pub epoch: u64,
    pub coin: Coin,
    pub value_commit: pallas::Point,
}

impl ConsensusMintRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![self.epoch.into(), self.coin.inner(), *valcom_coords.x(), *valcom_coords.y()]
    }
}

pub fn create_consensus_mint_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &ConsensusMintOutputInfo,
) -> Result<(Proof, ConsensusMintRevealed)> {
    let epoch_pallas = pallas::Base::from(output.epoch);
    let value_pallas = pallas::Base::from(output.value);
    let value_commit = pedersen_commitment_u64(output.value, output.value_blind);
    let (pub_x, pub_y) = output.public_key.xy();

    let coin = Coin::from(poseidon_hash([
        pub_x,
        pub_y,
        value_pallas,
        epoch_pallas,
        output.serial,
        output.coin_blind,
    ]));

    let public_inputs = ConsensusMintRevealed { epoch: output.epoch, coin, value_commit };

    let prover_witnesses = vec![
        Witness::Base(Value::known(pub_x)),
        Witness::Base(Value::known(pub_y)),
        Witness::Base(Value::known(value_pallas)),
        Witness::Base(Value::known(epoch_pallas)),
        Witness::Base(Value::known(output.serial)),
        Witness::Base(Value::known(output.coin_blind)),
        Witness::Scalar(Value::known(output.value_blind)),
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
    input: &ConsensusBurnInputInfo,
) -> Result<(Proof, ConsensusBurnRevealed, SecretKey)> {
    let nullifier = Nullifier::from(poseidon_hash([input.secret.inner(), input.note.serial]));
    let epoch = input.note.epoch;
    let epoch_pallas = pallas::Base::from(epoch);
    let value_pallas = pallas::Base::from(input.note.value);
    let value_commit = pedersen_commitment_u64(input.note.value, input.value_blind);
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
        Witness::Scalar(Value::known(input.value_blind)),
        Witness::Base(Value::known(input.secret.inner())),
        Witness::Uint32(Value::known(u64::from(input.leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(input.merkle_path.clone().try_into().unwrap())),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs, input.secret))
}
