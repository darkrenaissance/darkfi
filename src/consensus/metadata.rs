use super::{Participant, Vote};
use crate::util::serial::{SerialDecodable, SerialEncodable};

/// This struct represents [`Block`](super::Block) information used by the Ouroboros
/// Praos consensus protocol.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Metadata {
    /// Proof that the stakeholder is the block owner
    pub proof: String,
    /// Random seed for VRF
    pub rand_seed: String,
    /// Block owner signature
    pub signature: String,
}

impl Metadata {
    pub fn new(proof: String, rand_seed: String, signature: String) -> Self {
        Self { proof, rand_seed, signature }
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
