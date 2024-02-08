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

use std::collections::HashMap;

use darkfi::{
    blockchain::BlockchainOverlayPtr,
    tx::TransactionBuilder,
    validator::verification::verify_transaction,
    zk::{halo2::Value, Proof, ProvingKey, VerifyingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    ClientFailed, Result,
};
use darkfi_sdk::{
    bridgetree::{self, Hashable},
    crypto::{
        note::AeadEncryptedNote,
        pasta_prelude::{Curve, CurveAffine, Field},
        pedersen_commitment_u64, poseidon_hash, FuncId, Keypair, MerkleNode, MerkleTree, Nullifier,
        PublicKey, SecretKey,
    },
    pasta::pallas,
};
use log::{error, info};
use rand::rngs::OsRng;

use crate::{
    client::{compute_remainder_blind, Coin, MoneyNote, OwnCoin},
    model::{CoinAttributes, Input, MoneyFeeParamsV1, NullifierAttributes, Output, DARK_TOKEN_ID},
};

/// Append a fee-paying call to the given `TransactionBuilder`.
///
/// * `keypair`: Caller's keypair
/// * `coin`: `OwnCoin` to use in this builder
/// * `tree`: Merkle tree of coins used to create inclusion proofs
/// * `fee_zkbin`: `Fee_V1` zkas circuit ZkBinary
/// * `fee_pk`: `Fee_V1` zk circuit proving key
/// * `tx_builder`: `TransactionBuilder of the tx we want to pay fee for
/// * `overlay`: `BlockchainOverlayPtr` against which to verify the tx
/// * `time_keeper`: `TimeKeeper` needed for tx verification
/// * `verifying_keys`: ZK verifying keys needed for tx verification
#[allow(clippy::too_many_arguments)]
pub async fn append_fee_call(
    keypair: &Keypair,
    coin: &OwnCoin,
    tree: MerkleTree,
    fee_zkbin: &ZkBinary,
    fee_pk: &ProvingKey,
    tx_builder: &mut TransactionBuilder,
    overlay: &BlockchainOverlayPtr,
    verifying_block_height: u64,
    verifying_keys: &mut HashMap<[u8; 32], HashMap<String, VerifyingKey>>,
) -> Result<(MoneyFeeParamsV1, FeeCallSecrets)> {
    assert!(coin.note.value > 0);
    assert_eq!(coin.note.token_id, *DARK_TOKEN_ID);
    assert_eq!(coin.note.user_data, pallas::Base::ZERO);
    assert_eq!(coin.note.spend_hook, FuncId::none());

    // First we will verify the fee-less transaction to see how much gas
    // it uses for execution and verification.
    let tx = tx_builder.build()?;
    let gas_used =
        verify_transaction(overlay, verifying_block_height, &tx, verifying_keys, false).await?;

    // TODO: We could actually take a set of coins and then find one with
    //       enough value, instead of expecting one. It depends, the API
    //       is a bit weird.

    // TODO: FIXME: Proper fee pricing
    if coin.note.value < gas_used {
        error!(
            target: "money_contract::client::fee_v1",
            "Not enough value in given OwnCoin for fee, have {}, need {}",
            coin.note.value, gas_used,
        );
        return Err(ClientFailed::NotEnoughValue(coin.note.value).into())
    }

    let change_value = coin.note.value - gas_used;

    let input = FeeCallInput {
        leaf_position: coin.leaf_position,
        merkle_path: tree.witness(coin.leaf_position, 0).unwrap(),
        secret: coin.secret,
        note: coin.note.clone(),
        user_data_blind: pallas::Base::random(&mut OsRng),
    };

    let output = FeeCallOutput {
        public_key: keypair.public,
        value: change_value,
        token_id: coin.note.token_id,
        spend_hook: FuncId::none(),
        user_data: pallas::Base::ZERO,
        blind: pallas::Base::random(&mut OsRng),
    };

    let token_blind = pallas::Base::random(&mut OsRng);

    let input_value_blind = pallas::Scalar::random(&mut OsRng);
    let fee_value_blind = pallas::Scalar::random(&mut OsRng);
    let output_value_blind = compute_remainder_blind(&[], &[input_value_blind], &[fee_value_blind]);

    let signature_secret = SecretKey::random(&mut OsRng);

    info!(target: "money_contract::client::fee_v1", "Creating Fee_V1 ZK proof...");
    let (proof, public_inputs) = create_fee_proof(
        fee_zkbin,
        fee_pk,
        &input,
        input_value_blind,
        &output,
        output_value_blind,
        output.spend_hook,
        output.user_data,
        output.blind,
        token_blind,
        signature_secret,
    )?;

    // Encrypted note for the output
    let note = MoneyNote {
        value: output.value,
        token_id: output.token_id,
        spend_hook: output.spend_hook,
        user_data: output.user_data,
        coin_blind: output.blind,
        value_blind: output_value_blind,
        token_blind,
        memo: vec![],
    };

    let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;

    let params = MoneyFeeParamsV1 {
        input: Input {
            value_commit: public_inputs.input_value_commit,
            token_commit: public_inputs.token_commit,
            nullifier: public_inputs.nullifier,
            merkle_root: public_inputs.merkle_root,
            user_data_enc: public_inputs.input_user_data_enc,
            signature_public: public_inputs.signature_public,
        },
        output: Output {
            value_commit: public_inputs.output_value_commit,
            token_commit: public_inputs.token_commit,
            coin: public_inputs.output_coin,
            note: encrypted_note,
        },
        fee_value_blind,
        token_blind,
    };

    let secrets =
        FeeCallSecrets { proof, signature_secret, note, input_value_blind, output_value_blind };

    // TODO: Append call to tx builder

    Ok((params, secrets))
}

/// Private values related to the Fee call
pub struct FeeCallSecrets {
    /// The ZK proof created in this builder
    pub proof: Proof,
    /// The ephemeral secret key created for tx signining
    pub signature_secret: SecretKey,
    /// Decrypted note associated with the output
    pub note: MoneyNote,
    /// The value blind created for the input
    pub input_value_blind: pallas::Scalar,
    /// The value blind created for the output
    pub output_value_blind: pallas::Scalar,
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
    /// Input's spend hook
    pub input_spend_hook: FuncId,
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
            self.input_spend_hook.inner(),
            *sigpub_coords.x(),
            *sigpub_coords.y(),
            self.output_coin.inner(),
            *output_vc_coords.x(),
            *output_vc_coords.y(),
        ]
    }
}

struct FeeCallInput {
    leaf_position: bridgetree::Position,
    merkle_path: Vec<MerkleNode>,
    secret: SecretKey,
    note: MoneyNote,
    user_data_blind: pallas::Base,
}

type FeeCallOutput = CoinAttributes;

/// Create the `Fee_V1` ZK proof given parameters
#[allow(clippy::too_many_arguments)]
fn create_fee_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &FeeCallInput,
    input_value_blind: pallas::Scalar,
    output: &FeeCallOutput,
    output_value_blind: pallas::Scalar,
    output_spend_hook: FuncId,
    output_user_data: pallas::Base,
    output_coin_blind: pallas::Base,
    token_blind: pallas::Base,
    signature_secret: SecretKey,
) -> Result<(Proof, FeeRevealed)> {
    let public_key = PublicKey::from_secret(input.secret);
    let signature_public = PublicKey::from_secret(signature_secret);

    // Create input coin
    let input_coin = CoinAttributes {
        public_key,
        value: input.note.value,
        token_id: input.note.token_id,
        spend_hook: input.note.spend_hook,
        user_data: input.note.user_data,
        blind: input.note.coin_blind,
    }
    .to_coin();

    let nullifier =
        NullifierAttributes { secret_key: input.secret, coin: input_coin }.to_nullifier();

    let merkle_root = {
        let position: u64 = input.leaf_position.into();
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

    let input_user_data_enc = poseidon_hash([input.note.user_data, input.user_data_blind]);
    let input_value_commit = pedersen_commitment_u64(input.note.value, input_value_blind);
    let output_value_commit = pedersen_commitment_u64(output.value, output_value_blind);
    let token_commit = poseidon_hash([input.note.token_id.inner(), token_blind]);

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
        nullifier,
        input_value_commit,
        token_commit,
        merkle_root,
        input_spend_hook: input.note.spend_hook,
        input_user_data_enc,
        signature_public,
        output_coin,
        output_value_commit,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(input.secret.inner())),
        Witness::Uint32(Value::known(u64::from(input.leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(input.merkle_path.clone().try_into().unwrap())),
        Witness::Base(Value::known(signature_secret.inner())),
        Witness::Base(Value::known(pallas::Base::from(input.note.value))),
        Witness::Scalar(Value::known(input_value_blind)),
        Witness::Base(Value::known(input.note.spend_hook.inner())),
        Witness::Base(Value::known(input.note.user_data)),
        Witness::Base(Value::known(input.note.coin_blind)),
        Witness::Base(Value::known(input.user_data_blind)),
        Witness::Base(Value::known(pallas::Base::from(output.value))),
        Witness::Base(Value::known(output_spend_hook.inner())),
        Witness::Base(Value::known(output_user_data)),
        Witness::Scalar(Value::known(output_value_blind)),
        Witness::Base(Value::known(output_coin_blind)),
        Witness::Base(Value::known(input.note.token_id.inner())),
        Witness::Base(Value::known(token_blind)),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
