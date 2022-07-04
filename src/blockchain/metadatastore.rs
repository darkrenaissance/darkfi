use crate::{
    consensus::{Block, StreamletMetadata, OuroborosMetadata},
    util::{
        serial::{deserialize, serialize},
        time::Timestamp,
    },
    Error, Result,
};

const SLED_STREAMLET_METADATA_TREE: &[u8] = b"_streamlet_metadata";
const SLED_OUROBOROS_METADATA_TREE: &[u8] = b"_ouroboros_metadata";

/// The `StreamletMetadataStore` is a `sled` tree storing all the blockchain's
/// blocks' metadata used by the Streamlet consensus protocol, where the key
/// is the block's hash, and the value is the serialized metadata.
#[derive(Clone)]
pub struct StreamletMetadataStore(sled::Tree);

impl StreamletMetadataStore {
    /// Opens a new or existing `StreamletMetadataStore` on the given sled database.
    pub fn new(db: &sled::Db, genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Result<Self> {
        let tree = db.open_tree(SLED_STREAMLET_METADATA_TREE)?;
        let store = Self(tree);

        // In case the store is empty, initialize it with the genesis block.
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

    /// Insert a slice of blockhashes and respective metadata into the store.
    /// With sled, the operation is done as a batch.
    /// The block hash is used as the key, and the metadata is used as value.
    pub fn insert(&self, hashes: &[blake3::Hash], metadatas: &[StreamletMetadata]) -> Result<()> {
        assert_eq!(hashes.len(), metadatas.len());
        let mut batch = sled::Batch::default();

        for (i, hash) in hashes.iter().enumerate() {
            batch.insert(hash.as_bytes(), serialize(&metadatas[i]));
        }

        self.0.apply_batch(batch)?;
        Ok(())
    }

    /// Check if the metadata store contains a given block hash
    pub fn contains(&self, hash: &blake3::Hash) -> Result<bool> {
        Ok(self.0.contains_key(hash.as_bytes())?)
    }

    /// Fetch given blockhashes from the store. The resulting vector contains
    /// `Option`, which is `Some` if the slot was found in the blockstore, and
    /// otherwise it is `None`, if it has not. The second parameter is a boolean
    /// which tells the function to fail in case at least one blockhash was not
    /// found.
    pub fn get(
        &self,
        hashes: &[blake3::Hash],
        strict: bool,
    ) -> Result<Vec<Option<StreamletMetadata>>> {
        let mut ret = Vec::with_capacity(hashes.len());

        for hash in hashes {
            if let Some(found) = self.0.get(hash.as_bytes())? {
                let sm = deserialize(&found)?;
                ret.push(Some(sm));
            } else {
                if strict {
                    let s = hash.to_hex().as_str().to_string();
                    return Err(Error::BlockMetadataNotFound(s))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Retrieve all metadata from the store in the form of a tuple
    /// (`hash`, `metadata`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(blake3::Hash, StreamletMetadata)>> {
        let mut hashes = vec![];

        for hash in self.0.iter() {
            let (key, value) = hash.unwrap();
            let hash_bytes: [u8; 32] = key.as_ref().try_into().unwrap();
            let m = deserialize(&value)?;
            hashes.push((hash_bytes.into(), m));
        }

        Ok(hashes)
    }
}


#[derive(Clone)]
pub struct OuroborosMetadataStore(sled::Tree);

impl OuroborosMetadataStore {
    /// Opens a new or existing `OuroborosMetadataStore` on the given sled database.
    pub fn new(db: &sled::Db, genesis_ts: Timestamp, genesis_data: blake3::Hash, eta: [u8;32]) -> Result<Self> {
        let tree = db.open_tree(SLED_OUROBOROS_METADATA_TREE)?;
        let store = Self(tree);

        // In case the store is empty, initialize it with the genesis block.
        if store.0.is_empty() {
            let genesis_block = Block::genesis_block(genesis_ts, genesis_data, eta);
            let genesis_hash = blake3::hash(&serialize(&genesis_block));

            let metadata = OuroborosMetadata {
                eta,
            };

            store.insert(&[genesis_hash], &[metadata])?;
        }

        Ok(store)
    }

    /// Insert a slice of blockhashes and respective metadata into the store.
    /// With sled, the operation is done as a batch.
    /// The block hash is used as the key, and the metadata is used as value.
    pub fn insert(&self, hashes: &[blake3::Hash], metadatas: &[OuroborosMetadata]) -> Result<()> {
        assert_eq!(hashes.len(), metadatas.len());
        let mut batch = sled::Batch::default();

        for (i, hash) in hashes.iter().enumerate() {
            batch.insert(hash.as_bytes(), serialize(&metadatas[i]));
        }

        self.0.apply_batch(batch)?;
        Ok(())
    }

    /// Check if the metadata store contains a given block hash
    pub fn contains(&self, hash: &blake3::Hash) -> Result<bool> {
        Ok(self.0.contains_key(hash.as_bytes())?)
    }

    /// Fetch given blockhashes from the store. The resulting vector contains
    /// `Option`, which is `Some` if the slot was found in the blockstore, and
    /// otherwise it is `None`, if it has not. The second parameter is a boolean
    /// which tells the function to fail in case at least one blockhash was not
    /// found.
    pub fn get(
        &self,
        hashes: &[blake3::Hash],
        strict: bool,
    ) -> Result<Vec<Option<OuroborosMetadata>>> {
        let mut ret = Vec::with_capacity(hashes.len());

        for hash in hashes {
            if let Some(found) = self.0.get(hash.as_bytes())? {
                let sm = deserialize(&found)?;
                ret.push(Some(sm));
            } else {
                if strict {
                    let s = hash.to_hex().as_str().to_string();
                    return Err(Error::BlockMetadataNotFound(s))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Retrieve all metadata from the store in the form of a tuple
    /// (`hash`, `metadata`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(blake3::Hash, OuroborosMetadata)>> {
        let mut hashes = vec![];

        for hash in self.0.iter() {
            let (key, value) = hash.unwrap();
            let hash_bytes: [u8; 32] = key.as_ref().try_into().unwrap();
            let m = deserialize(&value)?;
            hashes.push((hash_bytes.into(), m));
        }

        Ok(hashes)
    }
}
