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

use darkfi::{
    zk::{Proof, ProvingKey},
    zkas::ZkBinary,
    ClientFailed, Result,
};
use darkfi_sdk::{
    crypto::{
        note::AeadEncryptedNote, pasta_prelude::*, BaseBlind, Blind, MerkleNode, ScalarBlind,
        SecretKey,
    },
    pasta::pallas,
};
use log::debug;
use rand::rngs::OsRng;

use super::proof::{create_transfer_burn_proof, create_transfer_mint_proof};
use crate::{
    client::{compute_remainder_blind, MoneyNote, OwnCoin, TokenId},
    error::MoneyError,
    model::{CoinAttributes, Input, MoneyTransferParamsV1, Output},
};

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

pub struct TransferCallClearInput {
    pub value: u64,
    pub token_id: TokenId,
    pub signature_secret: SecretKey,
}

pub struct TransferCallInput {
    /// The [`OwnCoin`] containing necessary metadata to create an input
    pub coin: OwnCoin,
    /// Merkle path in the Money Merkle tree for `coin`
    pub merkle_path: Vec<MerkleNode>,
    // In the DAO all inputs must have the same user_data_enc and use the same blind
    // So support allowing the user to set their own blind.
    pub user_data_blind: BaseBlind,
}

pub type TransferCallOutput = CoinAttributes;

impl TransferCallBuilder {
    pub fn build(self) -> Result<(MoneyTransferParamsV1, TransferCallSecrets)> {
        debug!(target: "contract::money::client::transfer::build", "Building Money::TransferV1 contract call");
        if self.clear_inputs.is_empty() && self.inputs.is_empty() {
            return Err(
                ClientFailed::VerifyError(MoneyError::TransferMissingInputs.to_string()).into()
            )
        }

        let mut params = MoneyTransferParamsV1 { inputs: vec![], outputs: vec![] };
        let mut signature_secrets = vec![];
        let mut proofs = vec![];

        let token_blind = BaseBlind::random(&mut OsRng);
        let mut input_blinds = vec![];
        let mut output_blinds = vec![];

        debug!(target: "contract::money::client::transfer::build", "Building anonymous inputs");
        for (i, input) in self.inputs.iter().enumerate() {
            let value_blind = Blind::random(&mut OsRng);
            input_blinds.push(value_blind);

            let signature_secret = SecretKey::random(&mut OsRng);
            signature_secrets.push(signature_secret);

            debug!(target: "contract::money::client::transfer::build", "Creating transfer burn proof for input {}", i);
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
                user_data_enc: public_inputs.user_data_enc,
                signature_public: public_inputs.signature_public,
            });

            proofs.push(proof);
        }

        // This value_blind calc assumes there will always be at least a single output
        if self.outputs.is_empty() {
            return Err(
                ClientFailed::VerifyError(MoneyError::TransferMissingOutputs.to_string()).into()
            )
        }

        let mut output_notes = vec![];

        for (i, output) in self.outputs.iter().enumerate() {
            let value_blind = if i == self.outputs.len() - 1 {
                compute_remainder_blind(&input_blinds, &output_blinds)
            } else {
                Blind::random(&mut OsRng)
            };

            output_blinds.push(value_blind);

            debug!(target: "contract::money::client::transfer::build", "Creating transfer mint proof for output {}", i);
            let (proof, public_inputs) = create_transfer_mint_proof(
                &self.mint_zkbin,
                &self.mint_pk,
                output,
                value_blind,
                token_blind,
                output.spend_hook,
                output.user_data,
                output.blind,
            )?;

            proofs.push(proof);

            // Encrypted note
            let note = MoneyNote {
                value: output.value,
                token_id: output.token_id,
                spend_hook: output.spend_hook,
                user_data: output.user_data,
                coin_blind: output.blind,
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

pub struct TransferCallSecrets {
    /// The ZK proofs created in this builder
    pub proofs: Vec<Proof>,
    /// The ephemeral secret keys created for signing
    pub signature_secrets: Vec<SecretKey>,

    /// Decrypted notes associated with each output
    pub output_notes: Vec<MoneyNote>,

    /// The value blinds created for the inputs
    pub input_value_blinds: Vec<ScalarBlind>,
    /// The value blinds created for the outputs
    pub output_value_blinds: Vec<ScalarBlind>,
}

impl TransferCallSecrets {
    pub fn minted_coins(&self, params: &MoneyTransferParamsV1) -> Vec<OwnCoin> {
        let mut minted_coins = vec![];
        for (output, output_note) in params.outputs.iter().zip(self.output_notes.iter()) {
            minted_coins.push(OwnCoin {
                coin: output.coin,
                note: output_note.clone(),
                secret: SecretKey::from(pallas::Base::ZERO),
                leaf_position: 0.into(),
            });
        }
        minted_coins
    }
}
