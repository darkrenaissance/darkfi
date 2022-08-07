use super::{Participant, Vote};
use crate::{
    crypto::{address::Address, schnorr::Signature},
    util::serial::{SerialDecodable, SerialEncodable},
};

/// This struct represents [`Block`](super::Block) information used by the Ouroboros
/// Praos consensus protocol.
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Metadata {
    /// Proof that the stakeholder is the block owner
    pub proof: String,
    /// Random seed for VRF
    pub rand_seed: String,
    /// Block owner signature
    pub signature: Signature,
    /// Block owner address
    pub address: Address,
}

impl Metadata {
    pub fn new(proof: String, rand_seed: String, signature: Signature, address: Address) -> Self {
        Self { proof, rand_seed, signature, address }
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
