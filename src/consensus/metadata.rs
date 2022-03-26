use serde::{Deserialize, Serialize};
use std::io;

use super::{
    util::{get_current_time, Timestamp},
    vote::Vote,
};

use crate::{
    util::serial::{Decodable, Encodable},
    Result,
};

/// This struct represents additional Block information used by the consensus protocol.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Metadata {
    /// Block information used by Ouroboros consensus
    pub om: OuroborosMetadata,
    /// Block information used by Streamlet consensus
    pub sm: StreamletMetadata,
    /// Block recieval timestamp
    pub timestamp: Timestamp,
}

impl Metadata {
    pub fn new(proof: String, r: String, s: String) -> Metadata {
        Metadata {
            om: OuroborosMetadata::new(proof, r, s),
            sm: StreamletMetadata::new(),
            timestamp: get_current_time(),
        }
    }
}

impl Encodable for Metadata {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.om.encode(&mut s).unwrap();
        len += self.sm.encode(&mut s).unwrap();
        len += self.timestamp.encode(&mut s).unwrap();
        Ok(len)
    }
}

impl Decodable for Metadata {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let om = Decodable::decode(&mut d)?;
        let sm = Decodable::decode(&mut d)?;
        let timestamp = Decodable::decode(&mut d)?;
        Ok(Self { om, sm, timestamp })
    }
}

/// This struct represents Block information used by Ouroboros consensus protocol.
#[derive(Debug, Clone, Deserialize, Serialize)]
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

impl Encodable for OuroborosMetadata {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.proof.encode(&mut s).unwrap();
        len += self.r.encode(&mut s).unwrap();
        len += self.s.encode(&mut s).unwrap();
        Ok(len)
    }
}

impl Decodable for OuroborosMetadata {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let proof = Decodable::decode(&mut d)?;
        let r = Decodable::decode(&mut d)?;
        let s = Decodable::decode(&mut d)?;
        Ok(Self { proof, r, s })
    }
}

/// This struct represents Block information used by Streamlet consensus protocol.
#[derive(Debug, Clone, Deserialize, Serialize)]
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

impl Encodable for StreamletMetadata {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.votes.encode(&mut s).unwrap();
        len += self.notarized.encode(&mut s).unwrap();
        len += self.finalized.encode(&mut s).unwrap();
        Ok(len)
    }
}

impl Decodable for StreamletMetadata {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let votes = Decodable::decode(&mut d)?;
        let notarized = Decodable::decode(&mut d)?;
        let finalized = Decodable::decode(&mut d)?;
        Ok(Self { votes, notarized, finalized })
    }
}
