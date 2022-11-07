/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use darkfi_sdk::crypto::{schnorr::SchnorrSecret, MerkleNode, PublicKey, SecretKey};
use darkfi_serial::serialize;
use pasta_curves::group::ff::Field;
use rand::rngs::OsRng;

use super::{
    partial::{PartialTransaction, PartialTransactionClearInput, PartialTransactionInput},
    Transaction, TransactionClearInput, TransactionInput, TransactionOutput,
};
use crate::{
    crypto::{
        burn_proof::create_burn_proof,
        mint_proof::create_mint_proof,
        note::Note,
        proof::ProvingKey,
        types::{
            DrkCoinBlind, DrkSerial, DrkSpendHook, DrkTokenId, DrkUserData, DrkUserDataBlind,
            DrkValueBlind,
        },
    },
    Result,
};

pub struct TransactionBuilder {
    pub clear_inputs: Vec<TransactionBuilderClearInputInfo>,
    pub inputs: Vec<TransactionBuilderInputInfo>,
    pub outputs: Vec<TransactionBuilderOutputInfo>,
}

pub struct TransactionBuilderClearInputInfo {
    pub value: u64,
    pub token_id: DrkTokenId,
    pub signature_secret: SecretKey,
}

pub struct TransactionBuilderInputInfo {
    pub leaf_position: incrementalmerkletree::Position,
    pub merkle_path: Vec<MerkleNode>,
    pub secret: SecretKey,
    pub note: Note,
}

pub struct TransactionBuilderOutputInfo {
    pub value: u64,
    pub token_id: DrkTokenId,
    pub public: PublicKey,
}

impl TransactionBuilder {
    fn compute_remainder_blind(
        clear_inputs: &[PartialTransactionClearInput],
        input_blinds: &[DrkValueBlind],
        output_blinds: &[DrkValueBlind],
    ) -> DrkValueBlind {
        let mut total = DrkValueBlind::zero();

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

    pub fn build(self, mint_pk: &ProvingKey, burn_pk: &ProvingKey) -> Result<Transaction> {
        assert!(self.clear_inputs.len() + self.inputs.len() > 0);

        let mut clear_inputs = vec![];
        let token_blind = DrkValueBlind::random(&mut OsRng);
        for input in &self.clear_inputs {
            let signature_public = PublicKey::from_secret(input.signature_secret);
            let value_blind = DrkValueBlind::random(&mut OsRng);

            let clear_input = PartialTransactionClearInput {
                value: input.value,
                token_id: input.token_id,
                value_blind,
                token_blind,
                signature_public,
            };
            clear_inputs.push(clear_input);
        }

        let mut inputs = vec![];
        let mut input_blinds = vec![];
        let mut signature_secrets = vec![];
        for input in self.inputs {
            let value_blind = DrkValueBlind::random(&mut OsRng);
            input_blinds.push(value_blind);

            let signature_secret = SecretKey::random(&mut OsRng);

            // Disable composability for this old obselete API
            let spend_hook = DrkSpendHook::from(0);
            let user_data = DrkUserData::from(0);
            let user_data_blind = DrkUserDataBlind::random(&mut OsRng);

            let (proof, revealed) = create_burn_proof(
                burn_pk,
                input.note.value,
                input.note.token_id,
                value_blind,
                token_blind,
                input.note.serial,
                spend_hook,
                user_data,
                user_data_blind,
                input.note.coin_blind,
                input.secret,
                input.leaf_position,
                input.merkle_path,
                signature_secret,
            )?;

            // First we make the tx then sign after
            signature_secrets.push(signature_secret);

            let input = PartialTransactionInput { burn_proof: proof, revealed };
            inputs.push(input);
        }

        let mut outputs = vec![];
        let mut output_blinds = vec![];
        // This value_blind calc assumes there will always be at least a single output
        assert!(!self.outputs.is_empty());

        for (i, output) in self.outputs.iter().enumerate() {
            let value_blind = if i == self.outputs.len() - 1 {
                Self::compute_remainder_blind(&clear_inputs, &input_blinds, &output_blinds)
            } else {
                DrkValueBlind::random(&mut OsRng)
            };
            output_blinds.push(value_blind);

            let serial = DrkSerial::random(&mut OsRng);
            let coin_blind = DrkCoinBlind::random(&mut OsRng);

            // Disable composability for this old obselete API
            let spend_hook = DrkSpendHook::from(0);
            let user_data = DrkUserData::from(0);

            let (mint_proof, revealed) = create_mint_proof(
                mint_pk,
                output.value,
                output.token_id,
                value_blind,
                token_blind,
                serial,
                spend_hook,
                user_data,
                coin_blind,
                output.public,
            )?;

            // Encrypted note
            let note = Note {
                serial,
                value: output.value,
                token_id: output.token_id,
                coin_blind,
                value_blind,
                token_blind,
                memo: vec![],
            };

            let encrypted_note = note.encrypt(&output.public)?;

            let output = TransactionOutput { mint_proof, revealed, enc_note: encrypted_note };
            outputs.push(output);
        }

        let partial_tx = PartialTransaction { clear_inputs, inputs, outputs };
        let unsigned_tx_data = serialize(&partial_tx);

        let mut clear_inputs = vec![];
        for (input, info) in partial_tx.clear_inputs.into_iter().zip(self.clear_inputs) {
            let secret = info.signature_secret;
            let signature = secret.sign(&mut OsRng, &unsigned_tx_data);
            let input = TransactionClearInput::from_partial(input, signature);
            clear_inputs.push(input);
        }

        let mut inputs = vec![];
        for (input, signature_secret) in
            partial_tx.inputs.into_iter().zip(signature_secrets.into_iter())
        {
            let signature = signature_secret.sign(&mut OsRng, &unsigned_tx_data);
            let input = TransactionInput::from_partial(input, signature);
            inputs.push(input);
        }

        Ok(Transaction { clear_inputs, inputs, outputs: partial_tx.outputs })
    }
}
