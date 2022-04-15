use sled::Batch;

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
    pub fn insert(&self, blocks: &[blake3::Hash], metadatas: &[StreamletMetadata]) -> Result<()> {
        assert_eq!(blocks.len(), metadatas.len());
        let mut batch = Batch::default();

        for (i, hash) in blocks.iter().enumerate() {
            batch.insert(hash.as_bytes(), serialize(&metadatas[i]));
        }

        self.0.apply_batch(batch)?;
        Ok(())
    }
}
