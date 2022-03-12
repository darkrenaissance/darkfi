use std::time::Instant;

use super::vote::Vote;

/// This struct represents additional Block information used by the consensus protocol.
#[derive(Debug, Clone)]
pub struct Metadata {
    /// Block information used by Ouroboros consensus
    pub om: OuroborosMetadata,
    /// Block information used by Streamlet consensus
    pub sm: StreamletMetadata,
    /// Block creation timestamp
    pub timestamp: Instant,
}

impl Metadata {
    pub fn new(proof: String, r: String, s: String) -> Metadata {
        Metadata {
            om: OuroborosMetadata::new(proof, r, s),
            sm: StreamletMetadata::new(),
            timestamp: Instant::now(),
        }
    }
}

/// This struct represents Block information used by Ouroboros consensus protocol.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct StreamletMetadata {
    /// Epoch votes
    pub votes: Vec<Vote>,
    /// Block notarization flag
    pub notarized: bool,
    /// Block finalization flag
    pub finalized: bool,
}

impl StreamletMetadata {
    pub fn new() -> StreamletMetadata {
        StreamletMetadata { votes: Vec::new(), notarized: false, finalized: false }
    }
}
