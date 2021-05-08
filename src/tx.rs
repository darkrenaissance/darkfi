use std::io;
use bellman::groth16;
use bls12_381::Bls12;
use ff::Field;
use group::Group;
use rand::rngs::OsRng;

use crate::crypto::{
    create_mint_proof, load_params, save_params, setup_mint_prover, verify_mint_proof,
    MintRevealedValues,
    note::Note
};
use crate::serial::{Decodable, Encodable, VarInt};
use crate::error::{Error, Result};
use crate::impl_vec;

pub struct TransactionBuilder {
    pub clear_inputs: Vec<TransactionBuilderClearInputInfo>,
    pub outputs: Vec<TransactionBuilderOutputInfo>,
}

impl TransactionBuilder {
    fn compute_remainder_blind(
        clear_inputs: &Vec<TransactionClearInput>,
        output_blinds: &Vec<jubjub::Fr>,
    ) -> jubjub::Fr {
        let mut lhs_total = jubjub::Fr::zero();
        for input in clear_inputs {
            lhs_total += input.valcom_blind;
        }

        let mut rhs_total = jubjub::Fr::zero();
        for output_blind in output_blinds {
            rhs_total += output_blind;
        }

        lhs_total - rhs_total
    }

    pub fn build(self, mint_params: &groth16::Parameters<Bls12>) -> Transaction {
        let mut clear_inputs = vec![];
        for input in &self.clear_inputs {
            let valcom_blind: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
            let clear_input = TransactionClearInput {
                value: input.value,
                valcom_blind,
            };
            clear_inputs.push(clear_input);
        }

        let mut outputs = vec![];
        let mut output_blinds = vec![];
        for (i, output) in self.outputs.iter().enumerate() {
            let valcom_blind = if i == self.outputs.len() - 1 {
                Self::compute_remainder_blind(&clear_inputs, &output_blinds)
            } else {
                jubjub::Fr::random(&mut OsRng)
            };
            output_blinds.push(valcom_blind);

            let serial: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
            let coin_blind: jubjub::Fr = jubjub::Fr::random(&mut OsRng);

            let (mint_proof, revealed) = create_mint_proof(
                mint_params,
                output.value,
                valcom_blind,
                serial,
                coin_blind,
                output.public,
            );
            let output = TransactionOutput {
                mint_proof,
                revealed,
            };
            outputs.push(output);
        }

        Transaction {
            clear_inputs,
            outputs,
        }
    }
}

pub struct TransactionBuilderClearInputInfo {
    pub value: u64,
}

pub struct TransactionBuilderInputInfo {
    pub value: u64,
    pub serial: jubjub::Fr,
}

pub struct TransactionBuilderOutputInfo {
    pub value: u64,
    pub public: jubjub::SubgroupPoint,
}

pub struct Transaction {
    pub clear_inputs: Vec<TransactionClearInput>,
    pub outputs: Vec<TransactionOutput>,
}

impl Encodable for Transaction {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.clear_inputs.encode(&mut s)?;
        len += self.outputs.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for Transaction {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            clear_inputs: Decodable::decode(&mut d)?,
            outputs: Decodable::decode(d)?
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

    pub fn verify(&self, pvk: &groth16::PreparedVerifyingKey<Bls12>) -> bool {
        let mut valcom_total = jubjub::SubgroupPoint::identity();
        for input in &self.clear_inputs {
            valcom_total += Self::compute_value_commit(input.value, &input.valcom_blind);
        }
        for output in &self.outputs {
            if !verify_mint_proof(pvk, &output.mint_proof, &output.revealed) {
                return false;
            }
            valcom_total -= &output.revealed.value_commit;
        }

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
            valcom_blind: Decodable::decode(d)?
        })
    }
}

pub struct TransactionOutput {
    pub mint_proof: groth16::Proof<Bls12>,
    pub revealed: MintRevealedValues,
}

impl_vec!(TransactionOutput);

impl Encodable for TransactionOutput {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.mint_proof.encode(&mut s)?;
        len += self.revealed.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for TransactionOutput {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            mint_proof: Decodable::decode(&mut d)?,
            revealed: Decodable::decode(d)?
        })
    }
}

