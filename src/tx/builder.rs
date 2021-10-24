use bellman::groth16;
use bls12_381::Bls12;
use ff::Field;
use rand::rngs::OsRng;

use super::{
    partial::{PartialTransaction, PartialTransactionClearInput, PartialTransactionInput},
    Transaction, TransactionClearInput, TransactionInput, TransactionOutput,
};
use crate::crypto::{
    create_mint_proof, create_spend_proof, merkle::MerklePath, merkle_node::MerkleNode, note::Note,
    schnorr,
};
use crate::serial::Encodable;

pub struct TransactionBuilder {
    pub clear_inputs: Vec<TransactionBuilderClearInputInfo>,
    pub inputs: Vec<TransactionBuilderInputInfo>,
    pub outputs: Vec<TransactionBuilderOutputInfo>,
}

pub struct TransactionBuilderClearInputInfo {
    pub value: u64,
    pub token_id: jubjub::Fr,
    pub signature_secret: jubjub::Fr,
}

pub struct TransactionBuilderInputInfo {
    pub merkle_path: MerklePath<MerkleNode>,
    pub secret: jubjub::Fr,
    pub note: Note,
}

pub struct TransactionBuilderOutputInfo {
    pub value: u64,
    pub token_id: jubjub::Fr,
    pub public: jubjub::SubgroupPoint,
}

impl TransactionBuilder {
    fn compute_remainder_blind(
        clear_inputs: &[PartialTransactionClearInput],
        input_blinds: &[jubjub::Fr],
        output_blinds: &[jubjub::Fr],
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
        let token_commit_blind: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
        for input in &self.clear_inputs {
            let signature_public =
                zcash_primitives::constants::SPENDING_KEY_GENERATOR * input.signature_secret;

            let valcom_blind: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
            let clear_input = PartialTransactionClearInput {
                value: input.value,
                token_id: input.token_id,
                valcom_blind,
                token_commit_blind,
                signature_public,
            };
            clear_inputs.push(clear_input);
        }

        let mut inputs = vec![];
        let mut input_blinds = vec![];
        let mut signature_secrets = vec![];
        for input in &self.inputs {
            input_blinds.push(input.note.valcom_blind);

            let signature_secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);

            // make proof

            // TODO: Some stupid glue code. Need to sort this out
            let auth_path: Vec<(bls12_381::Scalar, bool)> = input
                .merkle_path
                .auth_path
                .iter()
                .map(|(node, b)| ((*node).into(), *b))
                .collect();

            let (proof, revealed) = create_spend_proof(
                spend_params,
                input.note.value,
                input.note.token_id,
                input.note.valcom_blind,
                token_commit_blind,
                input.note.serial,
                input.note.coin_blind,
                input.secret,
                auth_path,
                signature_secret,
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
                output.token_id,
                valcom_blind,
                token_commit_blind,
                serial,
                coin_blind,
                output.public,
            );

            // Encrypted note

            let note = Note {
                serial,
                value: output.value,
                token_id: output.token_id,
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

        Transaction {
            clear_inputs,
            inputs,
            outputs: partial_tx.outputs,
        }
    }
}
