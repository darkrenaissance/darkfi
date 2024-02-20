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

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    bridgetree::Hashable,
    crypto::{
        pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, BaseBlind, FuncId, MerkleNode,
        PublicKey, ScalarBlind, SecretKey,
    },
    pasta::pallas,
};
use log::debug;
use rand::rngs::OsRng;

use super::{TransferCallInput, TransferCallOutput};
use crate::model::{Coin, CoinAttributes, Nullifier, NullifierAttributes};

pub struct TransferMintRevealed {
    pub coin: Coin,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
}

impl TransferMintRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![self.coin.inner(), *valcom_coords.x(), *valcom_coords.y(), self.token_commit]
    }
}

pub struct TransferBurnRevealed {
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub nullifier: Nullifier,
    pub merkle_root: MerkleNode,
    pub spend_hook: FuncId,
    pub user_data_enc: pallas::Base,
    pub signature_public: PublicKey,
}

impl TransferBurnRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![
            self.nullifier.inner(),
            *valcom_coords.x(),
            *valcom_coords.y(),
            self.token_commit,
            self.merkle_root.inner(),
            self.user_data_enc,
            self.spend_hook.inner(),
            self.signature_public.x(),
            self.signature_public.y(),
        ]
    }
}

pub fn create_transfer_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &TransferCallInput,
    value_blind: ScalarBlind,
    token_blind: BaseBlind,
    signature_secret: SecretKey,
) -> Result<(Proof, TransferBurnRevealed)> {
    let public_key = PublicKey::from_secret(input.secret);
    let signature_public = PublicKey::from_secret(signature_secret);

    let coin = CoinAttributes {
        public_key,
        value: input.note.value,
        token_id: input.note.token_id,
        spend_hook: input.note.spend_hook,
        user_data: input.note.user_data,
        blind: input.note.coin_blind,
    }
    .to_coin();

    let nullifier = NullifierAttributes { secret_key: input.secret, coin }.to_nullifier();

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

    let user_data_enc = poseidon_hash([input.note.user_data, input.user_data_blind.inner()]);
    let value_commit = pedersen_commitment_u64(input.note.value, value_blind);
    let token_commit = poseidon_hash([input.note.token_id.inner(), token_blind.inner()]);

    let public_inputs = TransferBurnRevealed {
        value_commit,
        token_commit,
        nullifier,
        merkle_root,
        spend_hook: input.note.spend_hook,
        user_data_enc,
        signature_public,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(input.secret.inner())),
        Witness::Base(Value::known(pallas::Base::from(input.note.value))),
        Witness::Base(Value::known(input.note.token_id.inner())),
        Witness::Base(Value::known(input.note.spend_hook.inner())),
        Witness::Base(Value::known(input.note.user_data)),
        Witness::Base(Value::known(input.note.coin_blind.inner())),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_blind.inner())),
        Witness::Base(Value::known(input.user_data_blind.inner())),
        Witness::Uint32(Value::known(u64::from(input.leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(input.merkle_path.clone().try_into().unwrap())),
        Witness::Base(Value::known(signature_secret.inner())),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}

#[allow(clippy::too_many_arguments)]
pub fn create_transfer_mint_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &TransferCallOutput,
    value_blind: ScalarBlind,
    token_blind: BaseBlind,
    spend_hook: FuncId,
    user_data: pallas::Base,
    coin_blind: BaseBlind,
) -> Result<(Proof, TransferMintRevealed)> {
    let value_commit = pedersen_commitment_u64(output.value, value_blind);
    let token_commit = poseidon_hash([output.token_id.inner(), token_blind.inner()]);
    let (pub_x, pub_y) = output.public_key.xy();

    let coin = CoinAttributes {
        public_key: output.public_key,
        value: output.value,
        token_id: output.token_id,
        spend_hook,
        user_data,
        blind: coin_blind,
    };
    debug!("Created coin: {:?}", coin);
    let coin = coin.to_coin();

    let public_inputs = TransferMintRevealed { coin, value_commit, token_commit };

    let prover_witnesses = vec![
        Witness::Base(Value::known(pub_x)),
        Witness::Base(Value::known(pub_y)),
        Witness::Base(Value::known(pallas::Base::from(output.value))),
        Witness::Base(Value::known(output.token_id.inner())),
        Witness::Base(Value::known(spend_hook.inner())),
        Witness::Base(Value::known(user_data)),
        Witness::Base(Value::known(coin_blind.inner())),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_blind.inner())),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
