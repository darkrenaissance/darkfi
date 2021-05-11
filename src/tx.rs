use bellman::groth16;
use bls12_381::Bls12;
use ff::Field;
use group::Group;
use rand::rngs::OsRng;
use std::collections::HashMap;
use std::io;

use crate::crypto::{
    create_mint_proof, create_spend_proof, load_params, note::{Note, EncryptedNote}, save_params, schnorr,
    setup_mint_prover, setup_spend_prover, verify_mint_proof, verify_spend_proof,
    MintRevealedValues, SpendRevealedValues,
    merkle::CommitmentTree,
    coin::Coin
};
use crate::error::{Error, Result};
use crate::impl_vec;
use crate::serial::{Decodable, Encodable, VarInt};

pub struct TransactionBuilder {
    pub clear_inputs: Vec<TransactionBuilderClearInputInfo>,
    pub inputs: Vec<TransactionBuilderInputInfo>,
    pub outputs: Vec<TransactionBuilderOutputInfo>,
}

impl TransactionBuilder {
    fn compute_remainder_blind(
        clear_inputs: &Vec<TransactionClearInput>,
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
            let valcom_blind: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
            let clear_input = TransactionClearInput {
                value: input.value,
                valcom_blind,
            };
            clear_inputs.push(clear_input);
        }

        let mut inputs = vec![];
        let mut input_blinds = vec![];
        for input in &self.inputs {
            input_blinds.push(input.note.valcom_blind.clone());

            let signature_secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
            let signature_public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * signature_secret;

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

            // make signature
            // TODO...
            let signature_secret = schnorr::SecretKey(signature_secret);
            let signature = signature_secret.sign(b"XYZ");

            let input = TransactionInput {
                spend_proof: proof,
                revealed,
                signature
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
                enc_note: encrypted_note
            };
            outputs.push(output);
        }

        Transaction {
            clear_inputs,
            inputs,
            outputs,
        }
    }
}

pub struct TransactionBuilderClearInputInfo {
    pub value: u64,
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

pub struct Transaction {
    pub clear_inputs: Vec<TransactionClearInput>,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
}

impl Encodable for Transaction {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.clear_inputs.encode(&mut s)?;
        len += self.inputs.encode(&mut s)?;
        len += self.outputs.encode(s)?;
        Ok(len)
    }
}

impl Decodable for Transaction {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            clear_inputs: Decodable::decode(&mut d)?,
            inputs: Decodable::decode(&mut d)?,
            outputs: Decodable::decode(d)?,
        })
    }
}

impl Transaction {
    fn compute_value_commit(value: u64, blind: &jubjub::Fr) -> jubjub::SubgroupPoint {
        let value_commit = (zcash_primitives::constants::VALUE_COMMITMENT_VALUE_GENERATOR
            * jubjub::Fr::from(value))
            + (zcash_primitives::constants::VALUE_COMMITMENT_RANDOMNESS_GENERATOR * blind);
        value_commit
    }

    pub fn verify(&self,
                  mint_pvk: &groth16::PreparedVerifyingKey<Bls12>,
                  spend_pvk: &groth16::PreparedVerifyingKey<Bls12>,

                  ) -> bool {
        let mut valcom_total = jubjub::SubgroupPoint::identity();
        for input in &self.clear_inputs {
            valcom_total += Self::compute_value_commit(input.value, &input.valcom_blind);
        }
        for input in &self.inputs {
            if !verify_spend_proof(spend_pvk, &input.spend_proof, &input.revealed) {
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
        // TODO: Verify signatures

        valcom_total == jubjub::SubgroupPoint::identity()
    }
}

pub struct TransactionClearInput {
    pub value: u64,
    pub valcom_blind: jubjub::Fr,
}

impl_vec!(TransactionClearInput);

impl Encodable for TransactionClearInput {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.value.encode(&mut s)?;
        len += self.valcom_blind.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for TransactionClearInput {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            value: Decodable::decode(&mut d)?,
            valcom_blind: Decodable::decode(d)?,
        })
    }
}

pub struct TransactionInput {
    pub spend_proof: groth16::Proof<Bls12>,
    pub revealed: SpendRevealedValues,
    pub signature: schnorr::Signature,
}

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
