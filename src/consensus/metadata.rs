use crate::util::serial::{SerialDecodable, SerialEncodable};

use super::{participant::Participant, util::Timestamp, vote::Vote};

/// This struct represents additional Block information used by the consensus protocol.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Metadata {
    /// Block creation timestamp
    pub timestamp: Timestamp,
    /// Block information used by Ouroboros consensus
    pub om: OuroborosMetadata,
    /// Block information used by Streamlet consensus
    pub sm: StreamletMetadata,
}

impl Metadata {
    pub fn new(
        timestamp: Timestamp,
        proof: String,
        r: String,
        s: String,
        participants: Vec<Participant>,
    ) -> Metadata {
        Metadata {
            timestamp,
            om: OuroborosMetadata::new(proof, r, s),
            sm: StreamletMetadata::new(participants),
        }
    }
}

/// This struct represents Block information used by Ouroboros consensus protocol.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct OuroborosMetadata {
    /// Proof the stakeholder is the block owner
    pub proof: String,
    /// Random seed for VRF
    pub r: String,
    /// Block owner signature
    pub s: String,
}

impl OuroborosMetadata {
    pub fn new(proof: String, r: String, s: String) -> OuroborosMetadata {
        OuroborosMetadata { proof, r, s }
    }
}

/// This struct represents Block information used by Streamlet consensus protocol.
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
    pub fn new(participants: Vec<Participant>) -> StreamletMetadata {
        StreamletMetadata { votes: Vec::new(), notarized: false, finalized: false, participants }
    }
}
