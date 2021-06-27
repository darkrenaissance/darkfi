pub mod builder;
pub mod partial;

use bellman::groth16;
use bls12_381::Bls12;
use group::Group;
use std::io;

use self::partial::{PartialTransactionClearInput, PartialTransactionInput};
use crate::crypto::{
    note::EncryptedNote, schnorr, verify_mint_proof, verify_spend_proof, MintRevealedValues,
    SpendRevealedValues,
};
use crate::error::Result;
use crate::impl_vec;
use crate::serial::{Decodable, Encodable, VarInt};
use crate::state;

pub use self::builder::{
    TransactionBuilder, TransactionBuilderClearInputInfo, TransactionBuilderInputInfo,
    TransactionBuilderOutputInfo,
};

pub struct Transaction {
    pub clear_inputs: Vec<TransactionClearInput>,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
}

pub struct TransactionClearInput {
    pub value: u64,
    pub asset_id: u64,
    pub valcom_blind: jubjub::Fr,
    pub asset_commit_blind: jubjub::Fr,
    pub signature_public: jubjub::SubgroupPoint,
    pub signature: schnorr::Signature,
}

pub struct TransactionInput {
    pub spend_proof: groth16::Proof<Bls12>,
    pub revealed: SpendRevealedValues,
    pub signature: schnorr::Signature,
}

pub struct TransactionOutput {
    pub mint_proof: groth16::Proof<Bls12>,
    pub revealed: MintRevealedValues,
    pub enc_note: EncryptedNote,
}

impl Transaction {
    fn encode_without_signature<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.clear_inputs.encode_without_signature(&mut s)?;
        len += self.inputs.encode_without_signature(&mut s)?;
        len += self.outputs.encode(s)?;
        Ok(len)
    }

    fn compute_pedersen_commit(value: u64, blind: &jubjub::Fr) -> jubjub::SubgroupPoint {
        let value_commit = (zcash_primitives::constants::VALUE_COMMITMENT_VALUE_GENERATOR
            * jubjub::Fr::from(value))
            + (zcash_primitives::constants::VALUE_COMMITMENT_RANDOMNESS_GENERATOR * blind);
        value_commit
    }

    fn verify_asset_commitments(&self) -> bool {
        assert_ne!(self.outputs.len(), 0);
        let asset_commit_value = self.outputs[0].revealed.asset_commit;

        let mut failed = self.inputs.iter().any(|input| input.revealed.asset_commit != asset_commit_value);
        failed = failed || self.outputs.iter().any(|output| output.revealed.asset_commit != asset_commit_value);
        failed = failed || self.clear_inputs.iter().any(|input| Self::compute_pedersen_commit(input.asset_id, &input.asset_commit_blind) != asset_commit_value);
        !failed
    }

    pub fn verify(
        &self,
        mint_pvk: &groth16::PreparedVerifyingKey<Bls12>,
        spend_pvk: &groth16::PreparedVerifyingKey<Bls12>,
    ) -> state::VerifyResult<()> {
        let mut valcom_total = jubjub::SubgroupPoint::identity();
        for input in &self.clear_inputs {
            valcom_total += Self::compute_pedersen_commit(input.value, &input.valcom_blind);
        }
        for (i, input) in self.inputs.iter().enumerate() {
            if !verify_spend_proof(spend_pvk, &input.spend_proof, &input.revealed) {
                return Err(state::VerifyFailed::SpendProof(i));
            }
            valcom_total += &input.revealed.value_commit;
        }
        for (i, output) in self.outputs.iter().enumerate() {
            if !verify_mint_proof(mint_pvk, &output.mint_proof, &output.revealed) {
                return Err(state::VerifyFailed::SpendProof(i));
            }
            valcom_total -= &output.revealed.value_commit;
        }

        if valcom_total != jubjub::SubgroupPoint::identity() {
            return Err(state::VerifyFailed::MissingFunds);
        }

        // Verify asset commitments match
        if !self.verify_asset_commitments() {
            return Err(state::VerifyFailed::AssetMismatch);
        }

        // Verify signatures
        let mut unsigned_tx_data = vec![];
        self.encode_without_signature(&mut unsigned_tx_data)
            .expect("TODO handle this");
        for (i, input) in self.clear_inputs.iter().enumerate() {
            let public = schnorr::PublicKey(input.signature_public.clone());
            if !public.verify(&unsigned_tx_data[..], &input.signature) {
                return Err(state::VerifyFailed::ClearInputSignature(i));
            }
        }
        for (i, input) in self.inputs.iter().enumerate() {
            let public = schnorr::PublicKey(input.revealed.signature_public.clone());
            if !public.verify(&unsigned_tx_data[..], &input.signature) {
                return Err(state::VerifyFailed::InputSignature(i));
            }
        }

        Ok(())
    }
}

impl TransactionClearInput {
    fn from_partial(partial: PartialTransactionClearInput, signature: schnorr::Signature) -> Self {
        Self {
            value: partial.value,
            asset_id: partial.asset_id,
            valcom_blind: partial.valcom_blind,
            asset_commit_blind: partial.asset_commit_blind,
            signature_public: partial.signature_public,
            signature,
        }
    }

    fn encode_without_signature<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.value.encode(&mut s)?;
        len += self.asset_id.encode(&mut s)?;
        len += self.valcom_blind.encode(&mut s)?;
        len += self.asset_commit_blind.encode(&mut s)?;
        len += self.signature_public.encode(s)?;
        Ok(len)
    }
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

impl Encodable for TransactionClearInput {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.value.encode(&mut s)?;
        len += self.asset_id.encode(&mut s)?;
        len += self.valcom_blind.encode(&mut s)?;
        len += self.asset_commit_blind.encode(&mut s)?;
        len += self.signature_public.encode(&mut s)?;
        len += self.signature.encode(s)?;
        Ok(len)
    }
}

impl Decodable for TransactionClearInput {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            value: Decodable::decode(&mut d)?,
            asset_id: Decodable::decode(&mut d)?,
            valcom_blind: Decodable::decode(&mut d)?,
            asset_commit_blind: Decodable::decode(&mut d)?,
            signature_public: Decodable::decode(&mut d)?,
            signature: Decodable::decode(d)?,
        })
    }
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

trait EncodableWithoutSignature {
    fn encode_without_signature<S: io::Write>(&self, s: S) -> Result<usize>;
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
impl_vec_without_signature!(TransactionInput);
impl_vec!(TransactionClearInput);
impl_vec!(TransactionInput);
impl_vec!(TransactionOutput);
