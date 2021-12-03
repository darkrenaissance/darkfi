pub mod builder;
pub mod partial;

use std::io;

use log::debug;
use pasta_curves::group::Group;

use crate::{
    crypto::{
        keypair::PublicKey,
        mint_proof::verify_mint_proof,
        note::EncryptedNote,
        proof::{Proof, VerifyingKey},
        schnorr,
        schnorr::SchnorrPublic,
        spend_proof::verify_spend_proof,
        util::{mod_r_p, pedersen_commitment_scalar, pedersen_commitment_u64},
        MintRevealedValues, SpendRevealedValues,
    },
    error::Result,
    impl_vec,
    serial::{Decodable, Encodable, VarInt},
    state,
    types::{DrkTokenId, DrkValueBlind, DrkValueCommit},
};

pub use self::builder::{
    TransactionBuilder, TransactionBuilderClearInputInfo, TransactionBuilderInputInfo,
    TransactionBuilderOutputInfo,
};

pub struct Transaction {
    pub clear_inputs: Vec<TransactionClearInput>,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
}

#[derive(Debug)]
pub struct TransactionClearInput {
    pub value: u64,
    pub token_id: DrkTokenId,
    pub value_blind: DrkValueBlind,
    pub token_blind: DrkValueBlind,
    pub signature_public: PublicKey,
    pub signature: schnorr::Signature,
}

#[derive(Debug)]
pub struct TransactionInput {
    pub spend_proof: Proof,
    pub revealed: SpendRevealedValues,
    pub signature: schnorr::Signature,
}

#[derive(Debug)]
pub struct TransactionOutput {
    pub mint_proof: Proof,
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

    pub fn verify(
        &self,
        mint_pvk: &VerifyingKey,
        spend_pvk: &VerifyingKey,
    ) -> state::VerifyResult<()> {
        let mut valcom_total = DrkValueCommit::identity();

        for input in &self.clear_inputs {
            valcom_total += pedersen_commitment_u64(input.value, input.value_blind);
        }

        for (i, input) in self.inputs.iter().enumerate() {
            if verify_spend_proof(spend_pvk, input.spend_proof.clone(), &input.revealed).is_err() {
                debug!(target: "TX VERIFY", "Failed to verify Spend proof {}", i);
                return Err(state::VerifyFailed::SpendProof(i))
            }
            valcom_total += &input.revealed.value_commit;
        }

        for (i, output) in self.outputs.iter().enumerate() {
            if verify_mint_proof(mint_pvk, &output.mint_proof, &output.revealed).is_err() {
                debug!(target: "TX VERIFY", "Failed to verify Mint proof {}", i);
                return Err(state::VerifyFailed::MintProof(i))
            }
            valcom_total -= &output.revealed.value_commit;
        }

        if valcom_total != DrkValueCommit::identity() {
            debug!(target: "TX VERIFY", "Missing funds");
            return Err(state::VerifyFailed::MissingFunds)
        }

        // Verify token commitments match
        if !self.verify_token_commitments() {
            debug!(target: "TX VERIFY", "Asset mismatch");
            return Err(state::VerifyFailed::AssetMismatch)
        }

        // Verify signatures
        let mut unsigned_tx_data = vec![];
        self.encode_without_signature(&mut unsigned_tx_data).expect("TODO handle this");
        for (i, input) in self.clear_inputs.iter().enumerate() {
            let public = &input.signature_public;
            if !public.verify(&unsigned_tx_data[..], &input.signature) {
                debug!(target: "TX VERIFY", "Failed to verify Clear Input signature {}", i);
                return Err(state::VerifyFailed::ClearInputSignature(i))
            }
        }
        for (i, input) in self.inputs.iter().enumerate() {
            let public = &input.revealed.signature_public;
            if !public.verify(&unsigned_tx_data[..], &input.signature) {
                debug!(target: "TX VERIFY", "Failed to verify Input signature {}", i);
                return Err(state::VerifyFailed::InputSignature(i))
            }
        }

        Ok(())
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
        Self { spend_proof: partial.spend_proof, revealed: partial.revealed, signature }
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
        len += self.token_id.encode(&mut s)?;
        len += self.value_blind.encode(&mut s)?;
        len += self.token_blind.encode(&mut s)?;
        len += self.signature_public.encode(&mut s)?;
        len += self.signature.encode(s)?;
        Ok(len)
    }
}

impl Decodable for TransactionClearInput {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            value: Decodable::decode(&mut d)?,
            token_id: Decodable::decode(&mut d)?,
            value_blind: Decodable::decode(&mut d)?,
            token_blind: Decodable::decode(&mut d)?,
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
