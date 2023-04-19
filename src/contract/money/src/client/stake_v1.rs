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
use darkfi_sdk::{
    bridgetree,
    bridgetree::Hashable,
    crypto::{
        pasta_prelude::*, pedersen_commitment_base, pedersen_commitment_u64, poseidon_hash,
        MerkleNode, MerkleTree, Nullifier, PublicKey, SecretKey, DARK_TOKEN_ID,
    },
    pasta::pallas,
};
use log::{debug, info};
use rand::rngs::OsRng;

use crate::{
    client::{MoneyNote, OwnCoin},
    model::{Input, MoneyStakeParamsV1},
};

pub struct MoneyStakeCallDebris {
    pub params: MoneyStakeParamsV1,
    pub proofs: Vec<Proof>,
    pub signature_secret: SecretKey,
    pub value_blind: pallas::Scalar,
}

pub struct MoneyStakeBurnRevealed {
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Point,
    pub nullifier: Nullifier,
    pub merkle_root: MerkleNode,
    pub spend_hook: pallas::Base,
    pub user_data_enc: pallas::Base,
    pub signature_public: PublicKey,
}

impl MoneyStakeBurnRevealed {
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
            // TODO: Why is spend hook in the struct but not here?
            self.user_data_enc,
            *sigpub_coords.x(),
            *sigpub_coords.y(),
        ]
    }
}

pub struct TransactionBuilderInputInfo {
    pub leaf_position: bridgetree::Position,
    pub merkle_path: Vec<MerkleNode>,
    pub secret: SecretKey,
    pub note: MoneyNote,
}

/// Struct holding necessary information to build a `Money::StakeV1` contract call.
pub struct MoneyStakeCallBuilder {
    /// `OwnCoin` we're given to use in this builder
    pub coin: OwnCoin,
    /// Merkle tree of coins used to create inclusion proofs
    pub tree: MerkleTree,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `Burn_V1` zk circuit
    pub burn_pk: ProvingKey,
}

impl MoneyStakeCallBuilder {
    pub fn build(&self) -> Result<MoneyStakeCallDebris> {
        info!("Building Money::StakeV1 contract call");
        assert!(self.coin.note.value != 0);
        assert!(self.coin.note.token_id == *DARK_TOKEN_ID);

        debug!("Building Money::StakeV1 anonymous input");
        let leaf_position = self.coin.leaf_position;
        let merkle_path = self.tree.witness(leaf_position, 0).unwrap();
        let input = TransactionBuilderInputInfo {
            leaf_position,
            merkle_path,
            secret: self.coin.secret,
            note: self.coin.note.clone(),
        };

        // Create new random blinds and an ephemeral signature key
        let value_blind = pallas::Scalar::random(&mut OsRng);
        let token_blind = pallas::Scalar::random(&mut OsRng);
        let signature_secret = SecretKey::random(&mut OsRng);
        let user_data_blind = pallas::Base::random(&mut OsRng);

        info!("Building Money::Stake V1 Burn ZK proof");
        let (proof, public_inputs) = create_stake_burn_proof(
            &self.burn_zkbin,
            &self.burn_pk,
            &input,
            value_blind,
            token_blind,
            user_data_blind,
            signature_secret,
        )?;

        let input = Input {
            value_commit: public_inputs.value_commit,
            token_commit: public_inputs.token_commit,
            nullifier: public_inputs.nullifier,
            merkle_root: public_inputs.merkle_root,
            spend_hook: public_inputs.spend_hook,
            user_data_enc: public_inputs.user_data_enc,
            signature_public: public_inputs.signature_public,
        };

        // We now fill this with necessary stuff
        let params = MoneyStakeParamsV1 { token_blind, input };
        let proofs = vec![proof];

        // Now we should have all the params, zk proof, signature secret and token blind.
        // We return it all and let the caller deal with it.
        let debris = MoneyStakeCallDebris { params, proofs, signature_secret, value_blind };
        Ok(debris)
    }
}

pub fn create_stake_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &TransactionBuilderInputInfo,
    value_blind: pallas::Scalar,
    token_blind: pallas::Scalar,
    user_data_blind: pallas::Base,
    signature_secret: SecretKey,
) -> Result<(Proof, MoneyStakeBurnRevealed)> {
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

    let public_inputs = MoneyStakeBurnRevealed {
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
