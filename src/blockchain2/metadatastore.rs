use crate::{consensus2::StreamletMetadata, util::serial::serialize, Result};

const SLED_STREAMLET_METADATA_TREE: &[u8] = b"_streamlet_metadata";

pub struct StreamletMetadataStore(sled::Tree);

impl StreamletMetadataStore {
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_STREAMLET_METADATA_TREE)?;
        Ok(Self(tree))
    }

    /// Insert [`StreamletMetadata`] into the `MetadataStore`.
    /// The blockhash for the metadata is used as the key,
    /// where value is the serialized metadata.
    pub fn insert(&self, block: blake3::Hash, metadata: &StreamletMetadata) -> Result<()> {
        self.0.insert(block.as_bytes(), serialize(metadata))?;
        Ok(())
    }
}
