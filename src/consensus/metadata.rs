use super::{Participant, Vote};
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
    util::serial::{SerialDecodable, SerialEncodable},
    VerifyResult,
};

#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct StakeholderMetadata {
    /// Block owner signature
    pub signature: Signature,
    /// Block owner address
    pub address: Address,
}

impl Default for StakeholderMetadata {
    fn default() -> Self {
        let keypair = Keypair::random(&mut OsRng);
        let address = Address::from(keypair.public);
        let sign = Signature::dummy();
        Self { signature: sign, address }
    }
}

impl StakeholderMetadata {
    pub fn new(signature: Signature, address: Address) -> Self {
        Self { signature, address }
    }
}

/// wrapper over the Proof, for possiblity any metadata necessary in the future.
#[derive(Default, Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct TransactionLeadProof {
    /// leadership proof
    pub lead_proof: Proof,
}

impl TransactionLeadProof {
    pub fn new(pk: &ProvingKey, coin: LeadCoin) -> Self {
        let proof = lead_proof::create_lead_proof(pk, coin).unwrap();
        Self { lead_proof: proof }
    }

    pub fn verify(&self, vk: VerifyingKey, public_inputs: &[DrkCircuitField]) -> VerifyResult<()> {
        lead_proof::verify_lead_proof(&vk, &self.lead_proof, public_inputs)
    }
}

impl From<Proof> for TransactionLeadProof {
    fn from(proof: Proof) -> Self {
        Self { lead_proof: proof }
    }
}

/// This struct represents [`Block`](super::Block) information used by the Ouroboros
/// Praos consensus protocol.
#[derive(Default, Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct OuroborosMetadata {
    /// response of global random oracle, or it's emulation.
    pub eta: [u8; 32],
    /// stakeholder lead NIZK lead proof
    pub lead_proof: TransactionLeadProof,
}

impl OuroborosMetadata {
    pub fn new(eta: [u8; 32], lead_proof: TransactionLeadProof) -> Self {
        Self { eta, lead_proof }
    }
}

/// This struct represents [`Block`](super::Block) information used by the Streamlet
/// consensus protocol.
#[derive(Debug, Clone, Default, SerialEncodable, SerialDecodable)]
pub struct StreamletMetadata {
    /// Slot votes
    pub votes: Vec<Vote>,
    /// Block notarization flag
    pub notarized: bool,
    /// Block finalization flag
    pub finalized: bool,
    /// Nodes participated in the voting process
    pub participants: Vec<Participant>,
}

impl StreamletMetadata {
    pub fn new(participants: Vec<Participant>) -> Self {
        Self { votes: vec![], notarized: false, finalized: false, participants }
    }
}
