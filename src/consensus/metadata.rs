use crate::{
    util::serial::{deserialize, serialize, SerialDecodable, SerialEncodable},
    Result,
};

use super::{block::Block, participant::Participant, util::Timestamp, vote::Vote};

const SLED_STREAMLET_METADATA_TREE: &[u8] = b"_streamlet_metadata";

/// This struct represents additional Block information used by the consensus protocol.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Metadata {
    /// Block creation timestamp
    pub timestamp: Timestamp,
    /// Block information used by Ouroboros consensus
    pub om: OuroborosMetadata,
}

impl Metadata {
    pub fn new(timestamp: Timestamp, proof: String, r: String, s: String) -> Metadata {
        Metadata { timestamp, om: OuroborosMetadata::new(proof, r, s) }
    }
}

/// This struct represents Block information used by Ouroboros consensus protocol.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
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
pub struct StreamletMetadataStore(sled::Tree);

impl StreamletMetadataStore {
    pub fn new(db: &sled::Db, genesis: i64) -> Result<Self> {
        let tree = db.open_tree(SLED_STREAMLET_METADATA_TREE)?;
        let store = Self(tree);
        if store.0.is_empty() {
            // Genesis block record is generated.
            let block = blake3::hash(&serialize(&Block::genesis_block(genesis)));
            let metadata = StreamletMetadata {
                votes: vec![],
                notarized: true,
                finalized: true,
                participants: vec![],
            };
            store.insert(block, &metadata)?;
        }

        Ok(store)
    }

    /// Insert streamlet metadata into the store.
    /// The block hash for the metadata is used as the key, where value is the serialized metadata.
    pub fn insert(&self, block: blake3::Hash, metadata: &StreamletMetadata) -> Result<()> {
        self.0.insert(block.as_bytes(), serialize(metadata))?;
        Ok(())
    }

    /// Fetch given streamlet metadata from the store.
    /// The resulting vector contains `Option` which is `Some` if the metadata
    /// was found in the store, and `None`, if it has not.
    pub fn get(&self, hashes: &[blake3::Hash]) -> Result<Vec<Option<StreamletMetadata>>> {
        let mut ret: Vec<Option<StreamletMetadata>> = Vec::with_capacity(hashes.len());

        for i in hashes {
            if let Some(found) = self.0.get(i.as_bytes())? {
                let metadata = deserialize(&found)?;
                ret.push(Some(metadata));
            } else {
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Retrieve all streamlet metadata.
    /// Be carefull as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<Option<(blake3::Hash, StreamletMetadata)>>> {
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
