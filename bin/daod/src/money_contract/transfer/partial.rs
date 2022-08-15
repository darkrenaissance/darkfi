use darkfi::{
    crypto::{
        keypair::PublicKey,
        types::{DrkTokenId, DrkValueBlind},
        BurnRevealedValues, Proof,
    },
    util::serial::{SerialDecodable, SerialEncodable},
};

use crate::money_contract::transfer::validate::Output;

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct Partial {
    pub clear_inputs: Vec<PartialClearInput>,
    pub inputs: Vec<PartialInput>,
    pub outputs: Vec<Output>,
    //pub proofs: Vec<Proof>,
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct PartialClearInput {
    pub value: u64,
    pub token_id: DrkTokenId,
    pub value_blind: DrkValueBlind,
    pub token_blind: DrkValueBlind,
    pub signature_public: PublicKey,
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct PartialInput {
    // TODO: BUG BUG FIXME!!!
    //pub burn_proof: Proof,
    pub revealed: BurnRevealedValues,
}
