use sled::Batch;

use crate::{
    consensus2::{Block, StreamletMetadata, Timestamp},
    util::serial::{deserialize, serialize},
    Error, Result,
};

const SLED_STREAMLET_METADATA_TREE: &[u8] = b"_streamlet_metadata";

pub struct StreamletMetadataStore(sled::Tree);

impl StreamletMetadataStore {
    pub fn new(db: &sled::Db, genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Result<Self> {
        let tree = db.open_tree(SLED_STREAMLET_METADATA_TREE)?;
        let store = Self(tree);

        // In case the store is empty, add genesis metadata.
        if store.0.is_empty() {
            let genesis_block = Block::genesis_block(genesis_ts, genesis_data);
            let genesis_hash = blake3::hash(&serialize(&genesis_block));

            let metadata = StreamletMetadata {
                votes: vec![],
                notarized: true,
                finalized: true,
                participants: vec![],
            };

            store.insert(&[genesis_hash], &[metadata])?;
        }

        Ok(store)
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

    /// Retrieve `StreamletMetadata` by given blockhashes.
    pub fn get(
        &self,
        blockhashes: &[blake3::Hash],
        strict: bool,
    ) -> Result<Vec<Option<StreamletMetadata>>> {
        let mut ret = Vec::with_capacity(blockhashes.len());

        for i in blockhashes {
            if let Some(found) = self.0.get(i.as_bytes())? {
                let sm = deserialize(&found)?;
                ret.push(Some(sm));
            } else {
                if strict {
                    let s = i.to_hex().as_str().to_string();
                    return Err(Error::BlockMetadataNotFound(s))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Retrieve all `StreamletMetadata`.
    /// Be careful as this will try to lead everything in memory.
    pub fn get_all(&self) -> Result<Vec<Option<(blake3::Hash, StreamletMetadata)>>> {
        let mut metadata = vec![];
        let iterator = self.0.into_iter().enumerate();
        for (_, r) in iterator {
            let (k, v) = r.unwrap();
            let hash_bytes: [u8; 32] = k.as_ref().try_into().unwrap();
            let m = deserialize(&v)?;
            metadata.push(Some((hash_bytes.into(), m)));
        }

        Ok(metadata)
    }
}
