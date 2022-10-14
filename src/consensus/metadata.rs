use super::Participant;
use rand::rngs::OsRng;

use crate::{
    crypto::{
        address::Address,
        keypair::Keypair,
        lead_proof,
        leadcoin::LeadCoin,
        proof::{Proof, ProvingKey, VerifyingKey},
        schnorr::Signature,
        types::*,
    },
    serial::{SerialDecodable, SerialEncodable},
    VerifyResult,
};

/// This struct represents [`Block`](super::Block) information used by the consensus protocol.
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Metadata {
    /// Block owner signature
    pub signature: Signature,
    /// Block owner address
    pub address: Address,
    /// Response of global random oracle, or it's emulation.
    pub eta: [u8; 32],
    /// Leader NIZK proof
    pub proof: LeadProof,
    /// Nodes participating in the consensus process
    pub participants: Vec<Participant>,
}

impl Default for Metadata {
    fn default() -> Self {
        let keypair = Keypair::random(&mut OsRng);
        let address = Address::from(keypair.public);
        let signature = Signature::dummy();
        let eta: [u8; 32] = *blake3::hash(b"let there be dark!").as_bytes();
        let proof = LeadProof::default();
        let participants = vec![];
        Self { signature, address, eta, proof, participants }
    }
}

impl Metadata {
    pub fn new(
        signature: Signature,
        address: Address,
        eta: [u8; 32],
        proof: LeadProof,
        participants: Vec<Participant>,
    ) -> Self {
        Self { signature, address, eta, proof, participants }
    }
}

/// Wrapper over the Proof, for future additions.
#[derive(Default, Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct LeadProof {
    /// Leadership proof
    pub proof: Proof,
}

impl LeadProof {
    pub fn new(pk: &ProvingKey, coin: LeadCoin) -> Self {
        let proof = lead_proof::create_lead_proof(pk, coin).unwrap();
        Self { proof }
    }

    pub fn verify(&self, vk: VerifyingKey, public_inputs: &[DrkCircuitField]) -> VerifyResult<()> {
        lead_proof::verify_lead_proof(&vk, &self.proof, public_inputs)
    }
}

impl From<Proof> for LeadProof {
    fn from(proof: Proof) -> Self {
        Self { proof }
    }
}
