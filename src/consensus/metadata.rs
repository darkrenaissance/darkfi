use super::{Participant, Vote};
use crate::util::{
    serial::{SerialDecodable, SerialEncodable},
    time::Timestamp,
};

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
    pub fn new(timestamp: Timestamp, eta: [u8;32]) -> Self {
        Self { timestamp, om: OuroborosMetadata::new(eta) }
    }
}

/// This struct represents [`Block`](super::Block) information used by the Ouroboros
/// Praos consensus protocol.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct OuroborosMetadata {
    // response of global random oracle, or it's emulation.
    pub eta: [u8;32],
}

impl OuroborosMetadata {
    pub fn new(eta: [u8;32]) -> Self {
        Self { eta }
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
