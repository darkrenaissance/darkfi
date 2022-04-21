use sled::Batch;

use crate::{
    consensus2::StreamletMetadata,
    util::serial::{deserialize, serialize},
    Error, Result,
};

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
