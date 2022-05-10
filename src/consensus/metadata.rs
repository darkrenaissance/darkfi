use super::{Participant, Vote};
use crate::util::{
    serial::{SerialDecodable, SerialEncodable},
    time::Timestamp,
};

/// This struct represents additional [`Block`] information used by
/// the consensus protocol
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Metadata {
    /// Block creation timestamp
    pub timestamp: Timestamp,
    /// Block information used by the Ouroboros Praos consensus
    pub om: OuroborosMetadata,
}

impl Metadata {
    pub fn new(timestamp: Timestamp, proof: String, r: String, s: String) -> Self {
        Self { timestamp, om: OuroborosMetadata::new(proof, r, s) }
    }
}

/// This struct represents [`Block`] information used by the Ouroboros
/// Praos consensus protocol.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct OuroborosMetadata {
    /// Proof that the stakeholder is the block owner
    pub proof: String,
    /// Random seed for VRF
    pub r: String,
    /// Block owner signature
    pub s: String,
}

impl OuroborosMetadata {
    pub fn new(proof: String, r: String, s: String) -> Self {
        Self { proof, r, s }
    }
}

/// This struct represents [`Block`] information used by the Streamlet
/// consensus protocol.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct StreamletMetadata {
    /// Epoch votes
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
