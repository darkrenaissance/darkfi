use super::{Participant, Vote};
use crate::{
    util::{
        serial::{SerialDecodable, SerialEncodable},
        time::Timestamp,
    },
    crypto::{
        proof::{
            Proof,
            ProvingKey,
            VerifyingKey,
        },
        lead_proof,
        leadcoin::LeadCoin,
    },
    Result, VerifyFailed, VerifyResult,
};

use log::error;

/// This struct represents additional [`Block`](super::Block) information used by
/// the consensus protocol
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Metadata {
    /// Block creation timestamp
    pub timestamp: Timestamp,
    /// Block information used by the Ouroboros Praos consensus
    pub om: OuroborosMetadata,
}

impl Metadata {
    pub fn new(timestamp: Timestamp, eta: [u8;32], lead_proof: TransactionLeadProof) -> Self {
        Self { timestamp, om: OuroborosMetadata::new(eta, lead_proof) }
    }
}


#[derive(Debug, Clone, PartialEq,  SerialEncodable, SerialDecodable)]
pub struct TransactionLeadProof {
    /// leadership proof
    pub lead_proof: Proof,
}

impl TransactionLeadProof {
    pub fn new(pk : ProvingKey, coin: LeadCoin) -> Self
    {
        let proof = lead_proof::create_lead_proof(pk.clone(), coin.clone()).unwrap();
        Self { lead_proof: proof }
    }

    pub fn verify(&self, vk : VerifyingKey, coin: LeadCoin) -> VerifyResult<()>
    {
        lead_proof::verify_lead_proof(&vk, &self.lead_proof, coin)
    }
}

impl Default for TransactionLeadProof {
    fn default() -> Self
    {
        Self {lead_proof: Proof::new(vec!())}
    }
}


/// This struct represents [`Block`](super::Block) information used by the Ouroboros
/// Praos consensus protocol.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct OuroborosMetadata {
    /// response of global random oracle, or it's emulation.
    pub eta: [u8;32],
    /// stakeholder lead NIZK lead proof
    pub lead_proof : TransactionLeadProof,
}

impl OuroborosMetadata {
    pub fn new(eta: [u8;32], lead_proof: TransactionLeadProof) -> Self {
        Self { eta, lead_proof }
    }
}

/// This struct represents [`Block`](super::Block) information used by the Streamlet
/// consensus protocol.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
