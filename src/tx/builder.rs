use bellman::groth16;
use bls12_381::Bls12;
use rand::rngs::OsRng;
use ff::Field;

use crate::serial::Encodable;
use super::{TransactionClearInput, TransactionInput, TransactionOutput, Transaction, partial::{PartialTransactionClearInput, PartialTransactionInput, PartialTransaction}};
use crate::crypto::{create_spend_proof, create_mint_proof, note::Note, schnorr};

pub struct TransactionBuilder {
    pub clear_inputs: Vec<TransactionBuilderClearInputInfo>,
    pub inputs: Vec<TransactionBuilderInputInfo>,
    pub outputs: Vec<TransactionBuilderOutputInfo>,
}

pub struct TransactionBuilderClearInputInfo {
    pub value: u64,
    pub signature_secret: jubjub::Fr,
}

pub struct TransactionBuilderInputInfo {
    pub merkle_path: Vec<(bls12_381::Scalar, bool)>,
    pub secret: jubjub::Fr,
    pub note: Note,
}

pub struct TransactionBuilderOutputInfo {
    pub value: u64,
    pub public: jubjub::SubgroupPoint,
}

impl TransactionBuilder {
    fn compute_remainder_blind(
        clear_inputs: &Vec<PartialTransactionClearInput>,
        input_blinds: &Vec<jubjub::Fr>,
        output_blinds: &Vec<jubjub::Fr>,
    ) -> jubjub::Fr {
        let mut total = jubjub::Fr::zero();

        for input in clear_inputs {
            total += input.valcom_blind;
        }

        for input_blind in input_blinds {
            total += input_blind;
        }

        for output_blind in output_blinds {
            total -= output_blind;
        }

        total
    }

    pub fn build(
        self,
        mint_params: &groth16::Parameters<Bls12>,
        spend_params: &groth16::Parameters<Bls12>,
    ) -> Transaction {
        let mut clear_inputs = vec![];
        for input in &self.clear_inputs {
            let signature_public =
                zcash_primitives::constants::SPENDING_KEY_GENERATOR * input.signature_secret;

            let valcom_blind: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
            let clear_input = PartialTransactionClearInput {
                value: input.value,
                valcom_blind,
                signature_public,
            };
            clear_inputs.push(clear_input);
        }

        let mut inputs = vec![];
        let mut input_blinds = vec![];
        let mut signature_secrets = vec![];
        for input in &self.inputs {
            input_blinds.push(input.note.valcom_blind.clone());

            let signature_secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);

            // make proof

            let (proof, revealed) = create_spend_proof(
                &spend_params,
                input.note.value,
                input.note.valcom_blind,
                input.note.serial,
                input.note.coin_blind,
                input.secret,
                input.merkle_path.clone(),
                signature_secret.clone(),
            );

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
            let valcom_blind = if i == self.outputs.len() - 1 {
                Self::compute_remainder_blind(&clear_inputs, &input_blinds, &output_blinds)
            } else {
                jubjub::Fr::random(&mut OsRng)
            };
            output_blinds.push(valcom_blind);

            let serial: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
            let coin_blind: jubjub::Fr = jubjub::Fr::random(&mut OsRng);

            let (mint_proof, revealed) = create_mint_proof(
                mint_params,
                output.value,
                valcom_blind.clone(),
                serial.clone(),
                coin_blind.clone(),
                output.public.clone(),
            );

            // Encrypted note

            let note = Note {
                serial,
                value: output.value,
                coin_blind,
                valcom_blind,
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
            let secret = schnorr::SecretKey(info.signature_secret.clone());
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

        Transaction {
            clear_inputs,
            inputs,
            outputs: partial_tx.outputs,
        }
    }
}

