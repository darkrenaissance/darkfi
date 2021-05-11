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
};
use crate::error::{Error, Result};
use crate::impl_vec;
use crate::serial::{Decodable, Encodable, VarInt};

pub trait CoinLookup {
    fn lookup(&self, coin: &[u8; 32]) -> CoinAttributes;
    fn add(&mut self, coin: [u8; 32], attrs: CoinAttributes);
}

#[derive(Clone)]
pub struct CoinAttributes {
    serial: jubjub::Fr,
    coin_blind: jubjub::Fr,
    value: u64,
}

pub struct CoinHashMap {
    map: HashMap<[u8; 32], CoinAttributes>,
}

impl CoinHashMap {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
}

impl CoinLookup for CoinHashMap {
    fn lookup(&self, coin: &[u8; 32]) -> CoinAttributes {
        self.map[coin].clone()
    }

    fn add(&mut self, coin: [u8; 32], attrs: CoinAttributes) {
        self.map.insert(coin, attrs);
    }
}

pub struct TransactionBuilder {
    pub clear_inputs: Vec<TransactionBuilderClearInputInfo>,
    pub inputs: Vec<TransactionBuilderInputInfo>,
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

    pub fn build<C: CoinLookup>(
        self,
        coin_look: &mut C,
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
        for input in &self.inputs {
            let valcom_blind: jubjub::Fr = jubjub::Fr::random(&mut OsRng);

            let signature_secret: jubjub::Fr = jubjub::Fr::random(&mut OsRng);
            let signature_public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * signature_secret;

            // make proof
            let attrs = coin_look.lookup(&input.coin);

            /*
            let (proof, revealed) = create_spend_proof(
                &spend_params,
                attrs.value,
                valcom_blind.clone(),
                attrs.serial,
                attrs.coin_blind,
                secret,
                merkle_path,
                signature_secret,
            );
            */

            // make signature
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

            let coin_attrs = CoinAttributes {
                serial: serial.clone(),
                coin_blind: coin_blind.clone(),
                value: output.value,
            };

            let (mint_proof, revealed) = create_mint_proof(
                mint_params,
                output.value,
                valcom_blind.clone(),
                serial.clone(),
                coin_blind.clone(),
                output.public.clone(),
            );

            coin_look.add(revealed.coin.clone(), coin_attrs);

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
