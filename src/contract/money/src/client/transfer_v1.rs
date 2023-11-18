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
    ClientFailed, Result,
};
use darkfi_sdk::{
    bridgetree,
    bridgetree::Hashable,
    crypto::{
        note::AeadEncryptedNote, pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, Keypair,
        MerkleNode, MerkleTree, Nullifier, PublicKey, SecretKey, TokenId,
    },
    pasta::pallas,
};
use log::{debug, error, info};
use rand::rngs::OsRng;

use crate::{
    client::{MoneyNote, OwnCoin},
    model::{ClearInput, Coin, Input, MoneyTransferParamsV1, Output},
};

pub struct TransferCallSecrets {
    /// The ZK proofs created in this builder
    pub proofs: Vec<Proof>,
    /// The ephemeral secret keys created for signing
    pub signature_secrets: Vec<SecretKey>,

    /// Decrypted notes associated with each output
    pub output_notes: Vec<MoneyNote>,

    /// The value blinds created for the inputs
    pub input_value_blinds: Vec<pallas::Scalar>,
    /// The value blinds created for the outputs
    pub output_value_blinds: Vec<pallas::Scalar>,
}

impl TransferCallSecrets {
    pub fn minted_coins(&self, params: &MoneyTransferParamsV1) -> Vec<OwnCoin> {
        let mut minted_coins = vec![];
        for (output, output_note) in params.outputs.iter().zip(self.output_notes.iter()) {
            minted_coins.push(OwnCoin {
                coin: output.coin,
                note: output_note.clone(),
                secret: SecretKey::from(pallas::Base::ZERO),
                nullifier: Nullifier::from(pallas::Base::ZERO),
                leaf_position: 0.into(),
            });
        }
        minted_coins
    }
}

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
    pub spend_hook: pallas::Base,
    pub user_data_enc: pallas::Base,
    pub signature_public: PublicKey,
}

impl TransferBurnRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().unwrap();
        let sigpub_coords = self.signature_public.inner().to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![
            self.nullifier.inner(),
            *valcom_coords.x(),
            *valcom_coords.y(),
            self.token_commit,
            self.merkle_root.inner(),
            self.user_data_enc,
            self.spend_hook,
            *sigpub_coords.x(),
            *sigpub_coords.y(),
        ]
    }
}

pub struct TransferCallClearInput {
    pub value: u64,
    pub token_id: TokenId,
    pub signature_secret: SecretKey,
}

pub struct TransferCallInput {
    pub leaf_position: bridgetree::Position,
    pub merkle_path: Vec<MerkleNode>,
    pub secret: SecretKey,
    pub note: MoneyNote,
    // In the DAO all inputs must have the same user_data_enc and use the same blind
    // So support allowing the user to set their own blind.
    pub user_data_blind: pallas::Base,
}

pub struct TransferCallOutput {
    pub value: u64,
    pub token_id: TokenId,
    pub public_key: PublicKey,
    pub spend_hook: pallas::Base,
    pub user_data: pallas::Base,
}

/// Struct holding necessary information to build a `Money::TransferV1` contract call.
pub struct TransferCallBuilder {
    /// Clear inputs
    pub clear_inputs: Vec<TransferCallClearInput>,
    /// Anonymous inputs
    pub inputs: Vec<TransferCallInput>,
    /// Anonymous outputs
    pub outputs: Vec<TransferCallOutput>,
    /// `Mint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `Mint_V1` zk circuit
    pub mint_pk: ProvingKey,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `Burn_V1` zk circuit
    pub burn_pk: ProvingKey,
}

impl TransferCallBuilder {
    fn compute_remainder_blind(
        clear_inputs: &[ClearInput],
        input_blinds: &[pallas::Scalar],
        output_blinds: &[pallas::Scalar],
    ) -> pallas::Scalar {
        let mut total = pallas::Scalar::zero();

        for input in clear_inputs {
            total += input.value_blind;
        }

        for input_blind in input_blinds {
            total += input_blind;
        }

        for output_blind in output_blinds {
            total -= output_blind;
        }

        total
    }

    pub fn build(self) -> Result<(MoneyTransferParamsV1, TransferCallSecrets)> {
        debug!("Building Money::TransferV1 contract call");
        assert!(self.clear_inputs.len() + self.inputs.len() > 0);

        let mut params =
            MoneyTransferParamsV1 { clear_inputs: vec![], inputs: vec![], outputs: vec![] };
        let mut signature_secrets = vec![];
        let mut proofs = vec![];

        let token_blind = pallas::Base::random(&mut OsRng);
        debug!("Building clear inputs");
        for input in self.clear_inputs {
            let signature_public = PublicKey::from_secret(input.signature_secret);
            let value_blind = pallas::Scalar::random(&mut OsRng);

            params.clear_inputs.push(ClearInput {
                value: input.value,
                token_id: input.token_id,
                value_blind,
                token_blind,
                signature_public,
            });
        }

        let mut input_blinds = vec![];
        let mut output_blinds = vec![];

        debug!("Building anonymous inputs");
        for (i, input) in self.inputs.iter().enumerate() {
            let value_blind = pallas::Scalar::random(&mut OsRng);
            input_blinds.push(value_blind);

            let signature_secret = SecretKey::random(&mut OsRng);
            signature_secrets.push(signature_secret);

            info!("Creating transfer burn proof for input {}", i);
            let (proof, public_inputs) = create_transfer_burn_proof(
                &self.burn_zkbin,
                &self.burn_pk,
                input,
                value_blind,
                token_blind,
                signature_secret,
            )?;

            params.inputs.push(Input {
                value_commit: public_inputs.value_commit,
                token_commit: public_inputs.token_commit,
                nullifier: public_inputs.nullifier,
                merkle_root: public_inputs.merkle_root,
                spend_hook: public_inputs.spend_hook,
                user_data_enc: public_inputs.user_data_enc,
                signature_public: public_inputs.signature_public,
            });

            proofs.push(proof);
        }

        // This value_blind calc assumes there will always be at least a single output
        assert!(!self.outputs.is_empty());

        let mut output_notes = vec![];

        for (i, output) in self.outputs.iter().enumerate() {
            let value_blind = if i == self.outputs.len() - 1 {
                Self::compute_remainder_blind(&params.clear_inputs, &input_blinds, &output_blinds)
            } else {
                pallas::Scalar::random(&mut OsRng)
            };

            output_blinds.push(value_blind);

            let serial = pallas::Base::random(&mut OsRng);

            info!("Creating transfer mint proof for output {}", i);
            let (proof, public_inputs) = create_transfer_mint_proof(
                &self.mint_zkbin,
                &self.mint_pk,
                output,
                value_blind,
                token_blind,
                serial,
                output.spend_hook,
                output.user_data,
            )?;

            proofs.push(proof);

            // Encrypted note
            let note = MoneyNote {
                serial,
                value: output.value,
                token_id: output.token_id,
                spend_hook: output.spend_hook,
                user_data: output.user_data,
                value_blind,
                token_blind,
                memo: vec![],
            };

            let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;
            output_notes.push(note);

            params.outputs.push(Output {
                value_commit: public_inputs.value_commit,
                token_commit: public_inputs.token_commit,
                coin: public_inputs.coin,
                note: encrypted_note,
            });
        }

        // Now we should have all the params, zk proofs, and signature secrets.
        // We return it all and let the caller deal with it.
        let secrets = TransferCallSecrets {
            proofs,
            signature_secrets,
            output_notes,
            input_value_blinds: input_blinds,
            output_value_blinds: output_blinds,
        };
        Ok((params, secrets))
    }
}

/// Select coins from `coins` of at least `min_value` in total.
/// Different strategies can be used. This function uses the dumb strategy
/// of selecting coins until we reach `min_value`.
pub fn select_coins(coins: Vec<OwnCoin>, min_value: u64) -> Result<(Vec<OwnCoin>, u64)> {
    let mut total_value = 0;
    let mut selected = vec![];

    for coin in coins {
        if total_value >= min_value {
            break
        }

        total_value += coin.note.value;
        selected.push(coin);
    }

    if total_value < min_value {
        error!("Not enough value to build tx inputs");
        return Err(ClientFailed::NotEnoughValue(total_value).into())
    }

    let change_value = total_value - min_value;

    Ok((selected, change_value))
}

/// Make a simple anonymous transfer call.
///
/// * `keypair`: Caller's keypair
/// * `recipient`: Recipient's public key
/// * `value`: Amount that we want to send to the recipient
/// * `token_id`: Token ID that we want to send to the recipient
/// * `coins`: Set of `OwnCoin` we're given to use in this builder
/// * `tree`: Merkle tree of coins used to create inclusion proofs
/// * `mint_zkbin`: `Mint_V1` zkas circuit ZkBinary
/// * `mint_pk`: Proving key for the `Mint_V1` zk circuit
/// * `burn_zkbin`: `Burn_V1` zkas circuit ZkBinary
/// * `burn_pk`: Proving key for the `Burn_V1` zk circuit
///
/// Returns a tuple of:
///
/// * The actual call data
/// * Secret values such as blinds
/// * A list of the spent coins
pub fn make_transfer_call(
    keypair: Keypair,
    recipient: PublicKey,
    value: u64,
    token_id: TokenId,
    coins: Vec<OwnCoin>,
    tree: MerkleTree,
    mint_zkbin: ZkBinary,
    mint_pk: ProvingKey,
    burn_zkbin: ZkBinary,
    burn_pk: ProvingKey,
) -> Result<(MoneyTransferParamsV1, TransferCallSecrets, Vec<OwnCoin>)> {
    debug!("Building Money::TransferV1 contract call");
    assert_ne!(value, 0);
    assert_ne!(token_id.inner(), pallas::Base::ZERO);
    assert!(!coins.is_empty());

    // Ensure the coins given to us are all of the same token ID.
    // The money contract base transfer doesn't allow conversions.
    for coin in &coins {
        assert_eq!(token_id, coin.note.token_id);
    }

    let mut inputs = vec![];
    let mut outputs = vec![];

    let (spent_coins, change_value) = select_coins(coins, value)?;

    for coin in spent_coins.iter() {
        let leaf_position = coin.leaf_position;
        let merkle_path = tree.witness(leaf_position, 0).unwrap();

        let input = TransferCallInput {
            leaf_position,
            merkle_path,
            secret: coin.secret,
            note: coin.note.clone(),
            user_data_blind: pallas::Base::random(&mut OsRng),
        };

        inputs.push(input);
    }
    debug!("Selected inputs");

    outputs.push(TransferCallOutput {
        value,
        token_id,
        public_key: recipient,
        spend_hook: pallas::Base::ZERO,
        user_data: pallas::Base::ZERO,
    });

    if change_value > 0 {
        outputs.push(TransferCallOutput {
            value: change_value,
            token_id,
            public_key: keypair.public,
            spend_hook: pallas::Base::ZERO,
            user_data: pallas::Base::ZERO,
        });
    }

    assert!(!inputs.is_empty());

    let xfer_builder = TransferCallBuilder {
        clear_inputs: vec![],
        inputs,
        outputs,
        mint_zkbin,
        mint_pk,
        burn_zkbin,
        burn_pk,
    };

    let (params, secrets) = xfer_builder.build()?;

    Ok((params, secrets, spent_coins))
}

pub fn create_transfer_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &TransferCallInput,
    value_blind: pallas::Scalar,
    token_blind: pallas::Base,
    signature_secret: SecretKey,
) -> Result<(Proof, TransferBurnRevealed)> {
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

    let user_data_enc = poseidon_hash([input.note.user_data, input.user_data_blind]);
    let value_commit = pedersen_commitment_u64(input.note.value, value_blind);
    let token_commit = poseidon_hash([input.note.token_id.inner(), token_blind]);

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
        Witness::Base(Value::known(pallas::Base::from(input.note.value))),
        Witness::Base(Value::known(input.note.token_id.inner())),
        Witness::Scalar(Value::known(value_blind)),
        Witness::Base(Value::known(token_blind)),
        Witness::Base(Value::known(input.note.serial)),
        Witness::Base(Value::known(input.note.spend_hook)),
        Witness::Base(Value::known(input.note.user_data)),
        Witness::Base(Value::known(input.user_data_blind)),
        Witness::Base(Value::known(input.secret.inner())),
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
    value_blind: pallas::Scalar,
    token_blind: pallas::Base,
    serial: pallas::Base,
    spend_hook: pallas::Base,
    user_data: pallas::Base,
) -> Result<(Proof, TransferMintRevealed)> {
    let value_commit = pedersen_commitment_u64(output.value, value_blind);
    let token_commit = poseidon_hash([output.token_id.inner(), token_blind]);
    let (pub_x, pub_y) = output.public_key.xy();

    let coin = Coin::from(poseidon_hash([
        pub_x,
        pub_y,
        pallas::Base::from(output.value),
        output.token_id.inner(),
        serial,
        spend_hook,
        user_data,
    ]));
    debug!("Created coin {:?}", coin);
    debug!("  pub_x: {:?}", pub_x);
    debug!("  pub_y: {:?}", pub_y);
    debug!("  value: {:?}", pallas::Base::from(output.value));
    debug!("  token_id: {:?}", output.token_id.inner());
    debug!("  serial: {:?}", serial);
    debug!("  spend_hook: {:?}", spend_hook);
    debug!("  user_data: {:?}", user_data);

    let public_inputs = TransferMintRevealed { coin, value_commit, token_commit };

    let prover_witnesses = vec![
        Witness::Base(Value::known(pub_x)),
        Witness::Base(Value::known(pub_y)),
        Witness::Base(Value::known(pallas::Base::from(output.value))),
        Witness::Base(Value::known(output.token_id.inner())),
        Witness::Base(Value::known(serial)),
        Witness::Base(Value::known(spend_hook)),
        Witness::Base(Value::known(user_data)),
        Witness::Scalar(Value::known(value_blind)),
        Witness::Base(Value::known(token_blind)),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
