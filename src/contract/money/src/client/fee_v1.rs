/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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
        pasta_prelude::{Curve, CurveAffine},
        pedersen_commitment_u64, poseidon_hash, BaseBlind, FuncId, MerkleNode, PublicKey,
        ScalarBlind, SecretKey,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;

use crate::{
    client::{Coin, MoneyNote, OwnCoin},
    model::{CoinAttributes, Nullifier},
};

/// Fixed gas used by the fee call.
/// This is the minimum gas any fee-paying transaction will use.
pub const FEE_CALL_GAS: u64 = 42_000_000;

/// Private values related to the Fee call
pub struct FeeCallSecrets {
    /// The ZK proof created in this builder
    pub proof: Proof,
    /// The ephemeral secret key created for tx signining
    pub signature_secret: SecretKey,
    /// Decrypted note associated with the output
    pub note: MoneyNote,
    /// The value blind created for the input
    pub input_value_blind: ScalarBlind,
    /// The value blind created for the output
    pub output_value_blind: ScalarBlind,
}

/// Revealed public inputs of the `Fee_V1` ZK proof
pub struct FeeRevealed {
    /// Input's Nullifier
    pub nullifier: Nullifier,
    /// Input's value commitment
    pub input_value_commit: pallas::Point,
    /// Token commitment
    pub token_commit: pallas::Base,
    /// Merkle root for input coin
    pub merkle_root: MerkleNode,
    /// Encrypted user data for input coin
    pub input_user_data_enc: pallas::Base,
    /// Public key used to sign transaction
    pub signature_public: PublicKey,
    /// Output coin commitment
    pub output_coin: Coin,
    /// Output value commitment
    pub output_value_commit: pallas::Point,
}

impl FeeRevealed {
    /// Transform the struct into a `Vec<pallas::Base>` ready for
    /// proof verification.
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let input_vc_coords = self.input_value_commit.to_affine().coordinates().unwrap();
        let output_vc_coords = self.output_value_commit.to_affine().coordinates().unwrap();
        let sigpub_coords = self.signature_public.inner().to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![
            self.nullifier.inner(),
            *input_vc_coords.x(),
            *input_vc_coords.y(),
            self.token_commit,
            self.merkle_root.inner(),
            self.input_user_data_enc,
            *sigpub_coords.x(),
            *sigpub_coords.y(),
            self.output_coin.inner(),
            *output_vc_coords.x(),
            *output_vc_coords.y(),
        ]
    }
}

pub struct FeeCallInput {
    /// The [`OwnCoin`] containing necessary metadata to create an input
    pub coin: OwnCoin,
    /// Merkle path in the Money Merkle tree for `coin`
    pub merkle_path: Vec<MerkleNode>,
    /// The blinding factor for user_data
    pub user_data_blind: BaseBlind,
}

pub type FeeCallOutput = CoinAttributes;

/// Create the `Fee_V1` ZK proof given parameters
#[allow(clippy::too_many_arguments)]
pub fn create_fee_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &FeeCallInput,
    input_value_blind: ScalarBlind,
    output: &FeeCallOutput,
    output_value_blind: ScalarBlind,
    output_spend_hook: FuncId,
    output_user_data: pallas::Base,
    output_coin_blind: BaseBlind,
    token_blind: BaseBlind,
    signature_secret: SecretKey,
) -> Result<(Proof, FeeRevealed)> {
    let public_key = PublicKey::from_secret(input.coin.secret);
    let signature_public = PublicKey::from_secret(signature_secret);

    // Create input coin
    let input_coin = CoinAttributes {
        public_key,
        value: input.coin.note.value,
        token_id: input.coin.note.token_id,
        spend_hook: input.coin.note.spend_hook,
        user_data: input.coin.note.user_data,
        blind: input.coin.note.coin_blind,
    }
    .to_coin();

    let merkle_root = {
        let position: u64 = input.coin.leaf_position.into();
        let mut current = MerkleNode::from(input_coin.inner());
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

    let input_user_data_enc =
        poseidon_hash([input.coin.note.user_data, input.user_data_blind.inner()]);
    let input_value_commit = pedersen_commitment_u64(input.coin.note.value, input_value_blind);
    let output_value_commit = pedersen_commitment_u64(output.value, output_value_blind);
    let token_commit = poseidon_hash([input.coin.note.token_id.inner(), token_blind.inner()]);

    // Create output coin
    let output_coin = CoinAttributes {
        public_key: output.public_key,
        value: output.value,
        token_id: output.token_id,
        spend_hook: output_spend_hook,
        user_data: output_user_data,
        blind: output_coin_blind,
    }
    .to_coin();

    let public_inputs = FeeRevealed {
        nullifier: input.coin.nullifier(),
        input_value_commit,
        token_commit,
        merkle_root,
        input_user_data_enc,
        signature_public,
        output_coin,
        output_value_commit,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(input.coin.secret.inner())),
        Witness::Uint32(Value::known(u64::from(input.coin.leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(input.merkle_path.clone().try_into().unwrap())),
        Witness::Base(Value::known(signature_secret.inner())),
        Witness::Base(Value::known(pallas::Base::from(input.coin.note.value))),
        Witness::Scalar(Value::known(input_value_blind.inner())),
        Witness::Base(Value::known(input.coin.note.spend_hook.inner())),
        Witness::Base(Value::known(input.coin.note.user_data)),
        Witness::Base(Value::known(input.coin.note.coin_blind.inner())),
        Witness::Base(Value::known(input.user_data_blind.inner())),
        Witness::Base(Value::known(pallas::Base::from(output.value))),
        Witness::Base(Value::known(output_spend_hook.inner())),
        Witness::Base(Value::known(output_user_data)),
        Witness::Scalar(Value::known(output_value_blind.inner())),
        Witness::Base(Value::known(output_coin_blind.inner())),
        Witness::Base(Value::known(input.coin.note.token_id.inner())),
        Witness::Base(Value::known(token_blind.inner())),
    ];

    //darkfi::zk::export_witness_json("proof/witness/fee_v1.json", &prover_witnesses, &public_inputs.to_vec());
    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
