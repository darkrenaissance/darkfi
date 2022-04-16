use sled::Batch;

use crate::{
    consensus2::{util::Timestamp, Block},
    util::serial::{deserialize, serialize},
    Result,
};

const SLED_BLOCK_TREE: &[u8] = b"_blocks";

pub struct BlockStore(sled::Tree);

impl BlockStore {
    /// Opens a new or existing `BlockStore` on the given sled database.
    pub fn new(db: &sled::Db, genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Result<Self> {
        let tree = db.open_tree(SLED_BLOCK_TREE)?;
        let store = Self(tree);

        // In case the store is empty, create the genesis block.
        if store.0.is_empty() {
            store.insert(&[Block::genesis_block(genesis_ts, genesis_data)])?;
        }

        Ok(store)
    }

    /// Insert a slice of [`Block`] into the blockstore. With sled, the
    /// operation is done as a batch.
    /// The blocks are hashed with BLAKE3 and this blockhash is used as
    /// the key, while value is the serialized block itself.
    pub fn insert(&self, blocks: &[Block]) -> Result<Vec<blake3::Hash>> {
        let mut ret = Vec::with_capacity(blocks.len());
        let mut batch = Batch::default();
        for i in blocks {
            let serialized = serialize(i);
            let blockhash = blake3::hash(&serialized);
            batch.insert(blockhash.as_bytes(), serialized);
            ret.push(blockhash);
        }

        self.0.apply_batch(batch)?;
        Ok(ret)
    }

    /// Fetch given blockhashes from the blockstore.
    /// The resulting vector contains `Option` which is `Some` if the block
    /// was found in the blockstore, and `None`, if it has not.
    pub fn get(&self, blockhashes: &[blake3::Hash]) -> Result<Vec<Option<Block>>> {
        let mut ret: Vec<Option<Block>> = Vec::with_capacity(blockhashes.len());

        for i in blockhashes {
            if let Some(found) = self.0.get(i.as_bytes())? {
                let block = deserialize(&found)?;
                ret.push(Some(block));
            } else {
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Check if the blockstore contains a given blockhash.
    pub fn contains(&self, blockhash: blake3::Hash) -> Result<bool> {
        Ok(self.0.contains_key(blockhash.as_bytes())?)
    }

    /// Retrieve all blocks.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<Option<(blake3::Hash, Block)>>> {
        let mut blocks = vec![];
        let iterator = self.0.into_iter().enumerate();
        for (_, r) in iterator {
            let (k, v) = r.unwrap();
            let hash_bytes: [u8; 32] = k.as_ref().try_into().unwrap();
            let block = deserialize(&v)?;
            blocks.push(Some((hash_bytes.into(), block)));
        }

        Ok(blocks)
    }
}
