use super::{Participant, Vote};
use rand::rngs::OsRng;

use crate::{
    util::{
        serial::{SerialDecodable, SerialEncodable},
        time::Timestamp,
    },
    crypto::{
        address::Address,
        schnorr::Signature,
        types::*,
        proof::{
            Proof,
            ProvingKey,
            VerifyingKey,
        },
        lead_proof,
        leadcoin::LeadCoin,
        keypair::Keypair,
    },
    Result, VerifyFailed, VerifyResult,
};

use log::error;


/*
/// This struct represents additional [`Block`](super::Block) information used by
/// the consensus protocol
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Metadata {
    /// Block creation timestamp
    pub timestamp: Timestamp,
    /// Block information used by the Ouroboros Praos consensus
    pub om: OuroborosMetadata,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            timestamp: Timestamp::current_time(),
            om: OuroborosMetadata::default(),
        }
    }
}

impl Metadata {
    pub fn new(timestamp: Timestamp, eta: [u8;32], lead_proof: TransactionLeadProof) -> Self {
        Self { timestamp, om: OuroborosMetadata::new(eta, lead_proof) }
    }
}
 */

#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
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
        Self {
            signature: sign,
            address: address,
        }
    }
}

impl StakeholderMetadata {
    pub fn new(signature: Signature, address: Address) -> Self {
        Self {
            signature,
            address
        }
    }
}

/// wrapper over the Proof, for possiblity any metadata necessary in the future.
#[derive(Debug, Clone, PartialEq,  SerialEncodable, SerialDecodable)]
pub struct TransactionLeadProof {
    /// leadership proof
    pub lead_proof: Proof,
}

impl Default for TransactionLeadProof {
    fn default() -> Self {
        Self {
            lead_proof : Proof::default(),
        }
    }
}

impl TransactionLeadProof {
    pub fn new(pk : &ProvingKey, coin: LeadCoin) -> Self
    {
        let proof = lead_proof::create_lead_proof(pk, coin.clone()).unwrap();
        Self { lead_proof: proof }
    }

    pub fn verify(&self, vk : VerifyingKey, public_inputs: &[DrkCircuitField]) -> VerifyResult<()>
    {
        lead_proof::verify_lead_proof(&vk, &self.lead_proof, public_inputs)
    }
}

impl From<Proof> for TransactionLeadProof {
    fn from(proof: Proof) -> Self {
        Self { lead_proof: proof}
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

impl Default for OuroborosMetadata {
    fn default() -> Self {
        Self {
            eta: [0;32],
            lead_proof: TransactionLeadProof::default(),
        }
    }
}

impl OuroborosMetadata {
    pub fn new(eta: [u8;32], lead_proof: TransactionLeadProof) -> Self {
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
