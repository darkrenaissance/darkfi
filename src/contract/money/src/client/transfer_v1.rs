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

/// Output metadata claimed from building a `Money::Transfer` call
pub struct TransferCallDebris {
    /// The parameters for `Money::Transfer` respective to this call
    pub params: MoneyTransferParamsV1,
    /// The ZK proofs created in this builder
    pub proofs: Vec<Proof>,
    /// The ephemeral secret keys created for signing
    pub signature_secrets: Vec<SecretKey>,
    /// The coins that have been spent in this builder
    pub spent_coins: Vec<OwnCoin>,
    /// The coins that have been minted in this builder
    pub minted_coins: Vec<OwnCoin>,
    /// The value blinds created for the inputs
    pub input_value_blinds: Vec<pallas::Scalar>,
    /// The value blinds created for the outputs
    pub output_value_blinds: Vec<pallas::Scalar>,
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

pub struct TransactionBuilderClearInputInfo {
    pub value: u64,
    pub token_id: TokenId,
    pub signature_secret: SecretKey,
}

pub struct TransactionBuilderInputInfo {
    pub leaf_position: bridgetree::Position,
    pub merkle_path: Vec<MerkleNode>,
    pub secret: SecretKey,
    pub note: MoneyNote,
}

pub struct TransactionBuilderOutputInfo {
    pub value: u64,
    pub token_id: TokenId,
    pub public_key: PublicKey,
}

/// Struct holding necessary information to build a `Money::TransferV1` contract call.
pub struct TransferCallBuilder {
    /// Caller's keypair
    pub keypair: Keypair,
    /// Recipient's public key
    pub recipient: PublicKey,
    /// Amount that we want to send to the recipient
    pub value: u64,
    /// Token ID that we want to send to the recipient
    pub token_id: TokenId,
    /// Spend hook for the recipient's output
    pub rcpt_spend_hook: pallas::Base,
    /// User data for the recipient's output
    pub rcpt_user_data: pallas::Base,
    /// User data blind for the recipient's output
    pub rcpt_user_data_blind: pallas::Base,
    /// Spend hook for the change output
    pub change_spend_hook: pallas::Base,
    /// User data for the change output
    pub change_user_data: pallas::Base,
    /// User data blind for the change output
    pub change_user_data_blind: pallas::Base,
    /// Set of `OwnCoin` we're given to use in this builder
    pub coins: Vec<OwnCoin>,
    /// Merkle tree of coins used to create inclusion proofs
    pub tree: MerkleTree,
    /// `Mint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `Mint_V1` zk circuit
    pub mint_pk: ProvingKey,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `Burn_V1` zk circuit
    pub burn_pk: ProvingKey,
    /// Marks if we want to build clear inputs instead of anonymous inputs
    pub clear_input: bool,
}

impl TransferCallBuilder {
    pub fn build(&self) -> Result<TransferCallDebris> {
        debug!("Building Money::TransferV1 contract call");
        assert!(self.value != 0);
        assert!(self.token_id.inner() != pallas::Base::zero());
        if !self.clear_input {
            assert!(!self.coins.is_empty());
        }

        // Ensure the coins given to us are all of the same token ID.
        // The money contract base transfer doesn't allow conversions.
        for coin in self.coins.iter() {
            assert_eq!(self.token_id, coin.note.token_id);
        }

        let mut clear_inputs = vec![];
        let mut inputs = vec![];
        let mut outputs = vec![];
        let mut change_outputs = vec![];
        let mut spent_coins = vec![];
        let mut minted_coins = vec![];
        let mut signature_secrets = vec![];
        let mut proofs = vec![];

        if self.clear_input {
            debug!("Building clear input");
            let input = TransactionBuilderClearInputInfo {
                value: self.value,
                token_id: self.token_id,
                signature_secret: self.keypair.secret,
            };

            clear_inputs.push(input);
        } else {
            debug!("Building anonymous inputs");
            let mut inputs_value = 0;

            for coin in self.coins.iter() {
                if inputs_value >= self.value {
                    debug!("inputs_value >= value");
                    break
                }

                let leaf_position = coin.leaf_position;
                let merkle_path = self.tree.witness(leaf_position, 0).unwrap();
                inputs_value += coin.note.value;

                let input = TransactionBuilderInputInfo {
                    leaf_position,
                    merkle_path,
                    secret: coin.secret,
                    note: coin.note.clone(),
                };

                inputs.push(input);
                spent_coins.push(coin.clone());
            }

            if inputs_value < self.value {
                error!("Not enough value to build tx inputs");
                return Err(ClientFailed::NotEnoughValue(inputs_value).into())
            }

            if inputs_value > self.value {
                let return_value = inputs_value - self.value;
                change_outputs.push(TransactionBuilderOutputInfo {
                    value: return_value,
                    token_id: self.token_id,
                    public_key: self.keypair.public,
                });
            }

            debug!("Finished building inputs");
        }

        outputs.push(TransactionBuilderOutputInfo {
            value: self.value,
            token_id: self.token_id,
            public_key: self.recipient,
        });

        assert!(clear_inputs.len() + inputs.len() > 0);

        // We now fill this with necessary stuff
        let mut params =
            MoneyTransferParamsV1 { clear_inputs: vec![], inputs: vec![], outputs: vec![] };

        let token_blind = pallas::Base::random(&mut OsRng);
        for input in clear_inputs {
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

        for (i, input) in inputs.iter().enumerate() {
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
                self.change_user_data_blind, // FIXME: We assume this, but it's just 1 usecase
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
        assert!(!outputs.is_empty());

        for (i, output) in change_outputs.iter().chain(outputs.iter()).enumerate() {
            let value_blind = if i == outputs.len() + change_outputs.len() - 1 {
                compute_remainder_blind(&params.clear_inputs, &input_blinds, &output_blinds)
            } else {
                pallas::Scalar::random(&mut OsRng)
            };

            output_blinds.push(value_blind);

            let serial = pallas::Base::random(&mut OsRng);

            let (scoped_sh, scoped_ud) = {
                if i >= change_outputs.len() {
                    (self.rcpt_spend_hook, self.rcpt_user_data)
                } else {
                    (self.change_spend_hook, self.change_user_data)
                }
            };

            info!("Creating transfer mint proof for output {}", i);
            let (proof, public_inputs) = create_transfer_mint_proof(
                &self.mint_zkbin,
                &self.mint_pk,
                output,
                value_blind,
                token_blind,
                serial,
                scoped_sh,
                scoped_ud,
            )?;

            proofs.push(proof);

            // Encrypted note
            let note = MoneyNote {
                serial,
                value: output.value,
                token_id: output.token_id,
                spend_hook: scoped_sh,
                user_data: scoped_ud,
                value_blind,
                token_blind,
                memo: vec![],
            };

            let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;

            minted_coins.push(OwnCoin {
                coin: public_inputs.coin,
                note,
                secret: SecretKey::from(pallas::Base::ZERO),
                nullifier: Nullifier::from(pallas::Base::ZERO),
                leaf_position: 0.into(),
            });

            params.outputs.push(Output {
                value_commit: public_inputs.value_commit,
                token_commit: public_inputs.token_commit,
                coin: public_inputs.coin,
                note: encrypted_note,
            });
        }

        // Now we should have all the params, zk proofs, and signature secrets.
        // We return it all and let the caller deal with it.
        let debris = TransferCallDebris {
            params,
            proofs,
            signature_secrets,
            spent_coins,
            minted_coins,
            input_value_blinds: input_blinds,
            output_value_blinds: output_blinds,
        };
        Ok(debris)
    }
}

pub fn create_transfer_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &TransactionBuilderInputInfo,
    value_blind: pallas::Scalar,
    token_blind: pallas::Base,
    user_data_blind: pallas::Base,
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

    let user_data_enc = poseidon_hash([input.note.user_data, user_data_blind]);
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
        Witness::Base(Value::known(user_data_blind)),
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
    output: &TransactionBuilderOutputInfo,
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
