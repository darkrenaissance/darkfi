use crate::{
    util::serial::{deserialize, serialize, SerialDecodable, SerialEncodable},
    Result,
};

use super::{participant::Participant, util::Timestamp, vote::Vote};

const SLED_METADATA_TREE: &[u8] = b"_metadata";

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

#[derive(Debug)]
pub struct MetadataStore(sled::Tree);

impl MetadataStore {
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_METADATA_TREE)?;
        Ok(Self(tree))
    }

    /// Insert metadata into the metadatastore.
    /// The block hash for the madatad is used as the key, where value is the serialized metadata.
    pub fn insert(&self, metadata: &Metadata, block: blake3::Hash) -> Result<()> {
        self.0.insert(block.as_bytes(), serialize(metadata))?;
        Ok(())
    }

    /// Retrieve all metadata.
    /// Be carefull as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<Option<(blake3::Hash, Metadata)>>> {
        let mut metadata = Vec::new();
        let mut iterator = self.0.into_iter().enumerate();
        while let Some((_, r)) = iterator.next() {
            let (k, v) = r.unwrap();
            let hash_bytes: [u8; 32] = k.as_ref().try_into().unwrap();
            let m = deserialize(&v)?;
            metadata.push(Some((hash_bytes.into(), m)));
        }
        Ok(metadata)
    }
}
