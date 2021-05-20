use bellman::groth16;
use bls12_381::Bls12;
use rand::rngs::OsRng;
use std::collections::HashMap;
use std::io;
use ff::Field;

use crate::error::{Error, Result};
use crate::impl_vec;
use crate::serial::{Decodable, Encodable, VarInt};
use super::{TransactionClearInput, TransactionInput, TransactionOutput, Transaction};
use crate::crypto::{
    coin::Coin,
    create_mint_proof, create_spend_proof, load_params,
    merkle::CommitmentTree,
    note::{EncryptedNote, Note},
    save_params, schnorr, setup_mint_prover, setup_spend_prover, verify_mint_proof,
    verify_spend_proof, MintRevealedValues, SpendRevealedValues,
};

pub struct PartialTransaction {
    pub clear_inputs: Vec<PartialTransactionClearInput>,
    pub inputs: Vec<PartialTransactionInput>,
    pub outputs: Vec<TransactionOutput>,
}

pub struct PartialTransactionClearInput {
    pub value: u64,
    pub valcom_blind: jubjub::Fr,
    pub signature_public: jubjub::SubgroupPoint,
}

pub struct PartialTransactionInput {
    pub spend_proof: groth16::Proof<Bls12>,
    pub revealed: SpendRevealedValues,
}

impl Encodable for PartialTransaction {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.clear_inputs.encode(&mut s)?;
        len += self.inputs.encode(&mut s)?;
        len += self.outputs.encode(s)?;
        Ok(len)
    }
}

impl Decodable for PartialTransaction {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            clear_inputs: Decodable::decode(&mut d)?,
            inputs: Decodable::decode(&mut d)?,
            outputs: Decodable::decode(d)?,
        })
    }
}

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

impl_vec!(PartialTransactionClearInput);
impl_vec!(PartialTransactionInput);

