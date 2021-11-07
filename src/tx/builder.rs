use pasta_curves::arithmetic::Field;
use rand::rngs::OsRng;

use super::{
    partial::{PartialTransaction, PartialTransactionClearInput, PartialTransactionInput},
    Transaction, TransactionClearInput, TransactionInput, TransactionOutput,
};
use crate::crypto::{
    mint_proof::create_mint_proof, note::Note, schnorr, spend_proof::create_spend_proof,
};
use crate::serial::Encodable;
use crate::{types::*, Result};

pub struct TransactionBuilder {
    pub clear_inputs: Vec<TransactionBuilderClearInputInfo>,
    pub inputs: Vec<TransactionBuilderInputInfo>,
    pub outputs: Vec<TransactionBuilderOutputInfo>,
}

pub struct TransactionBuilderClearInputInfo {
    pub value: u64,
    pub token_id: DrkTokenId,
    pub signature_secret: DrkSecretKey,
}

pub struct TransactionBuilderInputInfo {
    //pub merkle_path: MerklePath<MerkleNode>, // TODO:
    pub secret: DrkSecretKey,
    pub note: Note,
}

pub struct TransactionBuilderOutputInfo {
    pub value: u64,
    pub token_id: DrkTokenId,
    pub public: DrkPublicKey,
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

    pub fn build(self) -> Result<Transaction> {
        let mut clear_inputs = vec![];
        let token_blind = DrkValueBlind::random(&mut OsRng);
        for input in &self.clear_inputs {
            let signature_public = derive_public_key(input.signature_secret);
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
        for input in &self.inputs {
            input_blinds.push(input.note.value_blind);

            let signature_secret = DrkSecretKey::random(&mut OsRng);

            /*
            // TODO: Some stupid glue code. Need to sort this out
            let auth_path: Vec<(bls12_381::Scalar, bool)> = input
                .merkle_path
                .auth_path
                .iter()
                .map(|(node, b)| ((*node).into(), *b))
                .collect();
            */
            // TODO: FIXME:
            let auth_path = vec![DrkCoin::random(&mut OsRng)];

            let (proof, revealed) = create_spend_proof(
                input.note.value,
                input.note.token_id,
                input.note.value_blind,
                token_blind,
                input.note.serial,
                input.note.coin_blind,
                input.secret,
                auth_path,
                signature_secret,
            )?;

            // First we make the tx then sign after
            let signature_secret = schnorr::SecretKey(signature_secret);
            signature_secrets.push(signature_secret);

            let input = PartialTransactionInput {
                spend_proof: proof,
                revealed,
            };
            inputs.push(input);
        }

        let mut outputs = vec![];
        let mut output_blinds = vec![];

        for (i, output) in self.outputs.iter().enumerate() {
            let value_blind = if i == self.outputs.len() - 1 {
                Self::compute_remainder_blind(&clear_inputs, &input_blinds, &output_blinds)
            } else {
                DrkValueBlind::random(&mut OsRng)
            };
            output_blinds.push(value_blind);

            let serial = DrkSerial::random(&mut OsRng);
            let coin_blind = DrkCoinBlind::random(&mut OsRng);

            let (mint_proof, revealed) = create_mint_proof(
                output.value,
                output.token_id,
                value_blind,
                token_blind,
                serial,
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
            };

            let encrypted_note = note.encrypt(&output.public).unwrap();

            let output = TransactionOutput {
                mint_proof,
                revealed,
                enc_note: encrypted_note,
            };
            outputs.push(output);
        }

        let partial_tx = PartialTransaction {
            clear_inputs,
            inputs,
            outputs,
        };

        let mut unsigned_tx_data = vec![];
        partial_tx
            .encode(&mut unsigned_tx_data)
            .expect("TODO handle this");

        let mut clear_inputs = vec![];
        for (input, info) in partial_tx.clear_inputs.into_iter().zip(self.clear_inputs) {
            let secret = schnorr::SecretKey(info.signature_secret);
            let signature = secret.sign(&unsigned_tx_data[..]);
            let input = TransactionClearInput::from_partial(input, signature);
            clear_inputs.push(input);
        }

        let mut inputs = vec![];
        for (input, signature_secret) in partial_tx
            .inputs
            .into_iter()
            .zip(signature_secrets.into_iter())
        {
            let signature = signature_secret.sign(&unsigned_tx_data[..]);
            let input = TransactionInput::from_partial(input, signature);
            inputs.push(input);
        }

        Ok(Transaction {
            clear_inputs,
            inputs,
            outputs: partial_tx.outputs,
        })
    }
}
