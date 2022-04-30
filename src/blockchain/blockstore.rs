use log::debug;
use sled::Batch;

use crate::{
    consensus::{util::Timestamp, Block},
    util::serial::{deserialize, serialize},
    Error, Result,
};

const SLED_BLOCK_TREE: &[u8] = b"_blocks";
const SLED_BLOCK_ORDER_TREE: &[u8] = b"_block_order";

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
    pub fn get(&self, blockhashes: &[blake3::Hash], strict: bool) -> Result<Vec<Option<Block>>> {
        let mut ret = Vec::with_capacity(blockhashes.len());

        for i in blockhashes {
            if let Some(found) = self.0.get(i.as_bytes())? {
                let block = deserialize(&found)?;
                ret.push(Some(block));
            } else {
                if strict {
                    let s = i.to_hex().as_str().to_string();
                    return Err(Error::BlockNotFound(s))
                }
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

pub struct BlockOrderStore(sled::Tree);

impl BlockOrderStore {
    /// Opens a new or existing `BlockOderStore` on the given sled database.
    pub fn new(db: &sled::Db, genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Result<Self> {
        let tree = db.open_tree(SLED_BLOCK_ORDER_TREE)?;
        let store = Self(tree);

        // In case the store is empty, create the genesis block.
        if store.0.is_empty() {
            let block = Block::genesis_block(genesis_ts, genesis_data);
            let blockhash = blake3::hash(&serialize(&block));
            store.insert(&[block.sl], &[blockhash])?;
        }

        Ok(store)
    }

    /// Insert a slice of slots and blockhashes into the store.
    /// The block slot is used as the key, and the hash as value.
    pub fn insert(&self, slots: &[u64], hashes: &[blake3::Hash]) -> Result<()> {
        assert_eq!(slots.len(), hashes.len());
        let mut batch = Batch::default();
        for (i, sl) in slots.iter().enumerate() {
            batch.insert(&sl.to_be_bytes(), hashes[i].as_bytes());
        }

        self.0.apply_batch(batch)?;
        Ok(())
    }

    /// Retrieve all hashes given slots.
    pub fn get(&self, slots: &[u64], strict: bool) -> Result<Vec<Option<blake3::Hash>>> {
        //let mut ret = Vec::with_capacity(slots.len());
        let mut ret = vec![];

        for i in slots {
            if let Some(found) = self.0.get(i.to_be_bytes())? {
                let hash_bytes: [u8; 32] = found.as_ref().try_into().unwrap();
                let hash = blake3::Hash::from(hash_bytes);
                ret.push(Some(hash));
            } else {
                if strict {
                    debug!("BlockOrderStore::get() Slot {} not found", i);
                    return Err(Error::SlotNotFound(*i))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Retrieve n hashes after given slot.
    pub fn get_after(&self, slot: u64, n: u64) -> Result<Vec<blake3::Hash>> {
        let mut ret = vec![];

        let mut key = slot;
        let mut counter = 0;
        while counter <= n {
            if let Some(found) = self.0.get_gt(key.to_be_bytes())? {
                let key_bytes: [u8; 8] = found.0.as_ref().try_into().unwrap();
                key = u64::from_be_bytes(key_bytes);
                let block_hash = deserialize(&found.1)?;
                ret.push(block_hash);
                counter += 1;
            } else {
                break
            }
        }

        Ok(ret)
    }

    /// Retrieve the last block hash in the tree, based on the Ord
    /// implementation for Vec<u8>.
    pub fn get_last(&self) -> Result<Option<(u64, blake3::Hash)>> {
        if let Some(found) = self.0.last()? {
            let slot_bytes: [u8; 8] = found.0.as_ref().try_into().unwrap();
            let hash_bytes: [u8; 32] = found.1.as_ref().try_into().unwrap();
            let slot = u64::from_be_bytes(slot_bytes);
            let hash = blake3::Hash::from(hash_bytes);
            return Ok(Some((slot, hash)))
        }

        Ok(None)
    }

    /// Retrieve all block hashes.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<Option<(u64, blake3::Hash)>>> {
        let mut ret = vec![];
        let iterator = self.0.into_iter().enumerate();
        for (_, r) in iterator {
            let (k, v) = r.unwrap();
            let slot_bytes: [u8; 8] = k.as_ref().try_into().unwrap();
            let hash_bytes: [u8; 32] = v.as_ref().try_into().unwrap();
            let slot = u64::from_be_bytes(slot_bytes);
            let hash = blake3::Hash::from(hash_bytes);
            ret.push(Some((slot, hash)));
        }

        Ok(ret)
    }
}
