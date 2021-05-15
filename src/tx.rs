use bellman::groth16;
use bls12_381::Bls12;
use ff::Field;
use group::Group;
use rand::rngs::OsRng;
use std::collections::HashMap;
use std::io;

use crate::crypto::{
    coin::Coin,
    create_mint_proof, create_spend_proof, load_params,
    merkle::CommitmentTree,
    note::{EncryptedNote, Note},
    save_params, schnorr, setup_mint_prover, setup_spend_prover, verify_mint_proof,
    verify_spend_proof, MintRevealedValues, SpendRevealedValues,
};
use crate::error::{Error, Result};
use crate::impl_vec;
use crate::serial::{Decodable, Encodable, VarInt};

pub struct TransactionBuilder {
    pub clear_inputs: Vec<TransactionBuilderClearInputInfo>,
    pub inputs: Vec<TransactionBuilderInputInfo>,
    pub outputs: Vec<TransactionBuilderOutputInfo>,
    pub clear_outputs: Vec<TransactionBuilderClearOutputInfo>,
}

impl TransactionBuilder {
    fn compute_remainder_blind(
        clear_inputs: &Vec<PartialTransactionClearInput>,
        input_blinds: &Vec<jubjub::Fr>,
        output_blinds: &Vec<jubjub::Fr>,
        clear_outputs: &Vec<TransactionClearOutput>,
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

        for output in clear_outputs {
            total -= output.valcom_blind;
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

        let last_output_index = self.outputs.len() + self.clear_outputs.len() - 1;

        let mut outputs = vec![];
        let mut output_blinds = vec![];
        for (i, output) in self.outputs.iter().enumerate() {
            let valcom_blind = if i == last_output_index {
                Self::compute_remainder_blind(&clear_inputs, &input_blinds, &output_blinds, &vec![])
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

        let mut clear_outputs = vec![];

        for (i, output) in self.clear_outputs.into_iter().enumerate() {
            let valcom_blind = if self.outputs.len() + i == last_output_index {
                Self::compute_remainder_blind(&clear_inputs, &input_blinds, &output_blinds, &clear_outputs)
            } else {
                jubjub::Fr::random(&mut OsRng)
            };

            let output = TransactionClearOutput {
                value: output.value,
                valcom_blind,
                instructions: output.instructions
            };
            clear_outputs.push(output);
        }

        let partial_tx = PartialTransaction {
            clear_inputs,
            inputs,
            outputs,
            clear_outputs
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
            clear_outputs: partial_tx.clear_outputs
        }
    }
}

pub struct TransactionBuilderClearOutputInfo {
    pub value: u64,
    pub instructions: String
}

pub struct TransactionBuilderClearInputInfo {
    pub value: u64,
    pub signature_secret: jubjub::Fr,
}

pub struct TransactionBuilderInputInfo {
    pub coin: [u8; 32],
    pub merkle_path: Vec<(bls12_381::Scalar, bool)>,
    pub merkle_root: CommitmentTree<Coin>,
    pub secret: jubjub::Fr,
    pub note: Note,
}

pub struct TransactionBuilderOutputInfo {
    pub value: u64,
    pub public: jubjub::SubgroupPoint,
}

pub struct PartialTransaction {
    pub clear_inputs: Vec<PartialTransactionClearInput>,
    pub inputs: Vec<PartialTransactionInput>,
    pub outputs: Vec<TransactionOutput>,
    pub clear_outputs: Vec<TransactionClearOutput>,
}

impl Encodable for PartialTransaction {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.clear_inputs.encode(&mut s)?;
        len += self.inputs.encode(&mut s)?;
        len += self.outputs.encode(&mut s)?;
        len += self.clear_outputs.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for PartialTransaction {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            clear_inputs: Decodable::decode(&mut d)?,
            inputs: Decodable::decode(&mut d)?,
            outputs: Decodable::decode(&mut d)?,
            clear_outputs: Decodable::decode(&mut d)?,
        })
    }
}

pub struct Transaction {
    pub clear_inputs: Vec<TransactionClearInput>,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
    pub clear_outputs: Vec<TransactionClearOutput>
}

impl Encodable for Transaction {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.clear_inputs.encode(&mut s)?;
        len += self.inputs.encode(&mut s)?;
        len += self.outputs.encode(&mut s)?;
        len += self.clear_outputs.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for Transaction {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            clear_inputs: Decodable::decode(&mut d)?,
            inputs: Decodable::decode(&mut d)?,
            outputs: Decodable::decode(&mut d)?,
            clear_outputs: Decodable::decode(&mut d)?,
        })
    }
}

impl Transaction {
    fn encode_without_signature<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.clear_inputs.encode_without_signature(&mut s)?;
        len += self.inputs.encode_without_signature(&mut s)?;
        len += self.outputs.encode(&mut s)?;
        len += self.clear_outputs.encode(&mut s)?;
        Ok(len)
    }

    fn compute_value_commit(value: u64, blind: &jubjub::Fr) -> jubjub::SubgroupPoint {
        let value_commit = (zcash_primitives::constants::VALUE_COMMITMENT_VALUE_GENERATOR
            * jubjub::Fr::from(value))
            + (zcash_primitives::constants::VALUE_COMMITMENT_RANDOMNESS_GENERATOR * blind);
        value_commit
    }

    pub fn verify(
        &self,
        mint_pvk: &groth16::PreparedVerifyingKey<Bls12>,
        spend_pvk: &groth16::PreparedVerifyingKey<Bls12>,
    ) -> bool {
        let mut valcom_total = jubjub::SubgroupPoint::identity();
        for input in &self.clear_inputs {
            valcom_total += Self::compute_value_commit(input.value, &input.valcom_blind);
        }
        for input in &self.inputs {
            if !verify_spend_proof(spend_pvk, &input.spend_proof, &input.revealed) {
                println!("spend fail");
                return false;
            }
            valcom_total += &input.revealed.value_commit;
        }
        for output in &self.outputs {
            if !verify_mint_proof(mint_pvk, &output.mint_proof, &output.revealed) {
                println!("mint fail");
                return false;
            }
            valcom_total -= &output.revealed.value_commit;
        }
        for output in &self.clear_outputs {
            valcom_total -= Self::compute_value_commit(output.value, &output.valcom_blind);
        }

        // Verify signatures
        let mut unsigned_tx_data = vec![];
        self.encode_without_signature(&mut unsigned_tx_data)
            .expect("TODO handle this");
        for input in &self.clear_inputs {
            let public = schnorr::PublicKey(input.signature_public.clone());
            if !public.verify(&unsigned_tx_data[..], &input.signature) {
                return false;
            }
        }
        for input in &self.inputs {
            let public = schnorr::PublicKey(input.revealed.signature_public.clone());
            if !public.verify(&unsigned_tx_data[..], &input.signature) {
                return false;
            }
        }

        valcom_total == jubjub::SubgroupPoint::identity()
    }
}

pub struct TransactionClearInput {
    pub value: u64,
    pub valcom_blind: jubjub::Fr,
    pub signature_public: jubjub::SubgroupPoint,
    pub signature: schnorr::Signature,
}

impl TransactionClearInput {
    fn from_partial(partial: PartialTransactionClearInput, signature: schnorr::Signature) -> Self {
        Self {
            value: partial.value,
            valcom_blind: partial.valcom_blind,
            signature_public: partial.signature_public,
            signature,
        }
    }

    fn encode_without_signature<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.value.encode(&mut s)?;
        len += self.valcom_blind.encode(&mut s)?;
        len += self.signature_public.encode(s)?;
        Ok(len)
    }
}

macro_rules! impl_vec_without_signature {
    ($type: ty) => {
        impl EncodableWithoutSignature for Vec<$type> {
            #[inline]
            fn encode_without_signature<S: io::Write>(&self, mut s: S) -> Result<usize> {
                let mut len = 0;
                len += VarInt(self.len() as u64).encode(&mut s)?;
                for c in self.iter() {
                    len += c.encode_without_signature(&mut s)?;
                }
                Ok(len)
            }
        }
    };
}

impl_vec_without_signature!(TransactionClearInput);

impl_vec!(TransactionClearInput);

impl Encodable for TransactionClearInput {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.value.encode(&mut s)?;
        len += self.valcom_blind.encode(&mut s)?;
        len += self.signature_public.encode(&mut s)?;
        len += self.signature.encode(s)?;
        Ok(len)
    }
}

impl Decodable for TransactionClearInput {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            value: Decodable::decode(&mut d)?,
            valcom_blind: Decodable::decode(&mut d)?,
            signature_public: Decodable::decode(&mut d)?,
            signature: Decodable::decode(d)?,
        })
    }
}

pub struct PartialTransactionClearInput {
    pub value: u64,
    pub valcom_blind: jubjub::Fr,
    pub signature_public: jubjub::SubgroupPoint,
}

impl_vec!(PartialTransactionClearInput);

impl Encodable for PartialTransactionClearInput {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.value.encode(&mut s)?;
        len += self.valcom_blind.encode(&mut s)?;
        len += self.signature_public.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for PartialTransactionClearInput {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            value: Decodable::decode(&mut d)?,
            valcom_blind: Decodable::decode(&mut d)?,
            signature_public: Decodable::decode(&mut d)?,
        })
    }
}

pub struct PartialTransactionInput {
    pub spend_proof: groth16::Proof<Bls12>,
    pub revealed: SpendRevealedValues,
}

impl Encodable for PartialTransactionInput {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.spend_proof.encode(&mut s)?;
        len += self.revealed.encode(s)?;
        Ok(len)
    }
}

impl Decodable for PartialTransactionInput {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            spend_proof: Decodable::decode(&mut d)?,
            revealed: Decodable::decode(d)?,
        })
    }
}

impl_vec!(PartialTransactionInput);

pub struct TransactionInput {
    pub spend_proof: groth16::Proof<Bls12>,
    pub revealed: SpendRevealedValues,
    pub signature: schnorr::Signature,
}

impl TransactionInput {
    fn from_partial(partial: PartialTransactionInput, signature: schnorr::Signature) -> Self {
        Self {
            spend_proof: partial.spend_proof,
            revealed: partial.revealed,
            signature,
        }
    }

    fn encode_without_signature<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.spend_proof.encode(&mut s)?;
        len += self.revealed.encode(&mut s)?;
        Ok(len)
    }
}

trait EncodableWithoutSignature {
    fn encode_without_signature<S: io::Write>(&self, s: S) -> Result<usize>;
}

impl_vec_without_signature!(TransactionInput);

impl Encodable for TransactionInput {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.spend_proof.encode(&mut s)?;
        len += self.revealed.encode(&mut s)?;
        len += self.signature.encode(s)?;
        Ok(len)
    }
}

impl Decodable for TransactionInput {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            spend_proof: Decodable::decode(&mut d)?,
            revealed: Decodable::decode(&mut d)?,
            signature: Decodable::decode(d)?,
        })
    }
}

impl_vec!(TransactionInput);

pub struct TransactionOutput {
    pub mint_proof: groth16::Proof<Bls12>,
    pub revealed: MintRevealedValues,
    pub enc_note: EncryptedNote,
}

impl_vec!(TransactionOutput);

impl Encodable for TransactionOutput {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.mint_proof.encode(&mut s)?;
        len += self.revealed.encode(&mut s)?;
        len += self.enc_note.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for TransactionOutput {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            mint_proof: Decodable::decode(&mut d)?,
            revealed: Decodable::decode(&mut d)?,
            enc_note: Decodable::decode(&mut d)?,
        })
    }
}

pub struct TransactionClearOutput {
    pub value: u64,
    pub valcom_blind: jubjub::Fr,
    pub instructions: String
}

impl_vec!(TransactionClearOutput);

impl Encodable for TransactionClearOutput {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.value.encode(&mut s)?;
        len += self.valcom_blind.encode(&mut s)?;
        len += self.instructions.encode(s)?;
        Ok(len)
    }
}

impl Decodable for TransactionClearOutput {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            value: Decodable::decode(&mut d)?,
            valcom_blind: Decodable::decode(&mut d)?,
            instructions: Decodable::decode(&mut d)?,
        })
    }
}

