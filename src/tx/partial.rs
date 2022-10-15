use darkfi_serial::{SerialDecodable, SerialEncodable};

use super::TransactionOutput;
use crate::crypto::{
    keypair::PublicKey,
    types::{DrkTokenId, DrkValueBlind},
    BurnRevealedValues, Proof,
};

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct PartialTransaction {
    pub clear_inputs: Vec<PartialTransactionClearInput>,
    pub inputs: Vec<PartialTransactionInput>,
    pub outputs: Vec<TransactionOutput>,
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct PartialTransactionClearInput {
    pub value: u64,
    pub token_id: DrkTokenId,
    pub value_blind: DrkValueBlind,
    pub token_blind: DrkValueBlind,
    pub signature_public: PublicKey,
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct PartialTransactionInput {
    pub burn_proof: Proof,
    pub revealed: BurnRevealedValues,
}
