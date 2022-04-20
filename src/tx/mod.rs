use std::io;

use log::error;
use pasta_curves::group::Group;

use crate::{
    crypto::{
        burn_proof::verify_burn_proof,
        keypair::PublicKey,
        mint_proof::verify_mint_proof,
        note::EncryptedNote,
        proof::VerifyingKey,
        schnorr,
        schnorr::SchnorrPublic,
        types::{DrkTokenId, DrkValueBlind, DrkValueCommit},
        util::{mod_r_p, pedersen_commitment_scalar, pedersen_commitment_u64},
        BurnRevealedValues, MintRevealedValues, Proof,
    },
    impl_vec,
    util::serial::{Decodable, Encodable, SerialDecodable, SerialEncodable, VarInt},
    Result, VerifyFailed, VerifyResult,
};

pub mod builder;
mod partial;

/// A DarkFi transaction
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Transaction {
    /// Clear inputs
    pub clear_inputs: Vec<TransactionClearInput>,
    /// Anonymous inputs
    pub inputs: Vec<TransactionInput>,
    /// Anonymous outputs
    pub outputs: Vec<TransactionOutput>,
}

/// A transaction's clear input
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct TransactionClearInput {
    /// Input's value (amount)
    pub value: u64,
    /// Input's token ID
    pub token_id: DrkTokenId,
    /// Blinding factor for `value`
    pub value_blind: DrkValueBlind,
    /// Blinding factor for `token_id`
    pub token_blind: DrkValueBlind,
    /// Public key for the signature
    pub signature_public: PublicKey,
    /// Input's signature
    pub signature: schnorr::Signature,
}

/// A transaction's anonymous input
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct TransactionInput {
    /// Zero-knowledge proof for the input
    pub burn_proof: Proof,
    /// Public inputs for the zero-knowledge proof
    pub revealed: BurnRevealedValues,
    /// Input's signature
    pub signature: schnorr::Signature,
}

/// A transaction's anonymous output
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct TransactionOutput {
    /// Zero-knowledge proof for the output
    pub mint_proof: Proof,
    /// Public inputs for the zero-knowledge proof
    pub revealed: MintRevealedValues,
    /// The encrypted note
    pub enc_note: EncryptedNote,
}

impl Transaction {
    /// Verify the transaction
    pub fn verify(&self, mint_vk: &VerifyingKey, burn_vk: &VerifyingKey) -> VerifyResult<()> {
        // Accumulator for the value commitments
        let mut valcom_total = DrkValueCommit::identity();

        // Add values from the clear inputs
        for input in &self.clear_inputs {
            valcom_total += pedersen_commitment_u64(input.value, input.value_blind);
        }

        // Add values from the inputs
        for (i, input) in self.inputs.iter().enumerate() {
            if verify_burn_proof(burn_vk, &input.burn_proof, &input.revealed).is_err() {
                error!("tx::verify(): Failed to verify burn proof {}", i);
                return Err(VerifyFailed::BurnProof(i))
            }
            valcom_total += &input.revealed.value_commit;
        }

        // Subtract values from the outputs
        for (i, output) in self.outputs.iter().enumerate() {
            if verify_mint_proof(mint_vk, &output.mint_proof, &output.revealed).is_err() {
                error!("tx::verify(): Failed to verify mint proof {}", i);
                return Err(VerifyFailed::MintProof(i))
            }
            valcom_total -= &output.revealed.value_commit;
        }

        // If the accumulator is not back in its initial state,
        // there's a value mismatch.
        if valcom_total != DrkValueCommit::identity() {
            error!("tx::verify(): Missing funds");
            return Err(VerifyFailed::MissingFunds)
        }

        // Verify that the token commitments match
        if !self.verify_token_commitments() {
            error!("tx::verify(): Token ID mismatch");
            return Err(VerifyFailed::TokenMismatch)
        }

        // Verify the available signatures
        let mut unsigned_tx_data = vec![];
        self.encode_without_signature(&mut unsigned_tx_data)?;

        for (i, input) in self.clear_inputs.iter().enumerate() {
            let public = &input.signature_public;
            if !public.verify(&unsigned_tx_data[..], &input.signature) {
                error!("tx::verify(): Failed to verify Clear Input signature {}", i);
                return Err(VerifyFailed::ClearInputSignature(i))
            }
        }

        for (i, input) in self.inputs.iter().enumerate() {
            let public = &input.revealed.signature_public;
            if !public.verify(&unsigned_tx_data[..], &input.signature) {
                error!("tx::verify(): Failed to verify Input signature {}", i);
                return Err(VerifyFailed::InputSignature(i))
            }
        }

        Ok(())
    }

    fn encode_without_signature<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.clear_inputs.encode_without_signature(&mut s)?;
        len += self.inputs.encode_without_signature(&mut s)?;
        len += self.outputs.encode(s)?;
        Ok(len)
    }

    fn verify_token_commitments(&self) -> bool {
        assert_ne!(self.outputs.len(), 0);
        let token_commit_value = self.outputs[0].revealed.token_commit;

        let mut failed =
            self.inputs.iter().any(|input| input.revealed.token_commit != token_commit_value);

        failed = failed ||
            self.outputs.iter().any(|output| output.revealed.token_commit != token_commit_value);

        failed = failed ||
            self.clear_inputs.iter().any(|input| {
                pedersen_commitment_scalar(mod_r_p(input.token_id), input.token_blind) !=
                    token_commit_value
            });
        !failed
    }
}

impl TransactionClearInput {
    fn from_partial(
        partial: partial::PartialTransactionClearInput,
        signature: schnorr::Signature,
    ) -> Self {
        Self {
            value: partial.value,
            token_id: partial.token_id,
            value_blind: partial.value_blind,
            token_blind: partial.token_blind,
            signature_public: partial.signature_public,
            signature,
        }
    }

    fn encode_without_signature<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.value.encode(&mut s)?;
        len += self.token_id.encode(&mut s)?;
        len += self.value_blind.encode(&mut s)?;
        len += self.token_blind.encode(&mut s)?;
        len += self.signature_public.encode(s)?;
        Ok(len)
    }
}

impl TransactionInput {
    fn from_partial(
        partial: partial::PartialTransactionInput,
        signature: schnorr::Signature,
    ) -> Self {
        Self { burn_proof: partial.burn_proof, revealed: partial.revealed, signature }
    }

    fn encode_without_signature<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.burn_proof.encode(&mut s)?;
        len += self.revealed.encode(&mut s)?;
        Ok(len)
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
impl_vec!(Transaction);
