use darkfi_serial::{deserialize, serialize};

use crate::{
    consensus::{Block, Header},
    util::time::Timestamp,
    Error, Result,
};

const SLED_HEADER_TREE: &[u8] = b"_headers";
const SLED_BLOCK_TREE: &[u8] = b"_blocks";
const SLED_BLOCK_ORDER_TREE: &[u8] = b"_block_order";

/// The `HeaderStore` is a `sled` tree storing all the blockchain's blocks' headers
/// where the key is the headers's hash, and value is the serialized header.
#[derive(Clone)]
pub struct HeaderStore(sled::Tree);

impl HeaderStore {
    /// Opens a new or existing `HeaderStore` on the given sled database.
    pub fn new(db: &sled::Db, genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Result<Self> {
        let tree = db.open_tree(SLED_HEADER_TREE)?;
        let store = Self(tree);

        // In case the store is empty, initialize it with the genesis header.
        let genesis_header = Header::genesis_header(genesis_ts, genesis_data);
        if store.0.is_empty() {
            store.insert(&[genesis_header])?;
        }

        Ok(store)
    }

    /// Insert a slice of [`Header`] into the blockstore. With sled, the
    /// operation is done as a batch.
    /// The headers are hashed with BLAKE3 and this headerhash is used as
    /// the key, while value is the serialized [`Header`] itself.
    /// On success, the function returns the header hashes in the same order.
    pub fn insert(&self, headers: &[Header]) -> Result<Vec<blake3::Hash>> {
        let mut ret = Vec::with_capacity(headers.len());
        let mut batch = sled::Batch::default();

        for header in headers {
            let serialized = serialize(header);
            let headerhash = blake3::hash(&serialized);
            batch.insert(headerhash.as_bytes(), serialized);
            ret.push(headerhash);
        }

        self.0.apply_batch(batch)?;
        Ok(ret)
    }

    /// Check if the headerstore contains a given headerhash.
    pub fn contains(&self, headerhash: &blake3::Hash) -> Result<bool> {
        Ok(self.0.contains_key(headerhash.as_bytes())?)
    }

    /// Fetch given headerhashes from the headerstore.
    /// The resulting vector contains `Option`, which is `Some` if the header
    /// was found in the headerstore, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one header was not found.
    pub fn get(&self, headerhashes: &[blake3::Hash], strict: bool) -> Result<Vec<Option<Header>>> {
        let mut ret = Vec::with_capacity(headerhashes.len());

        for hash in headerhashes {
            if let Some(found) = self.0.get(hash.as_bytes())? {
                let header = deserialize(&found)?;
                ret.push(Some(header));
            } else {
                if strict {
                    let s = hash.to_hex().as_str().to_string();
                    return Err(Error::HeaderNotFound(s))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Retrieve all headers from the headerstore in the form of a tuple
    /// (`headerhash`, `header`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(blake3::Hash, Header)>> {
        let mut headers = vec![];

        for header in self.0.iter() {
            let (key, value) = header.unwrap();
            let hash_bytes: [u8; 32] = key.as_ref().try_into().unwrap();
            let header = deserialize(&value)?;
            headers.push((hash_bytes.into(), header));
        }

        Ok(headers)
    }
}

/// The `BlockStore` is a `sled` tree storing all the blockchain's blocks
/// where the key is the block's headers' hash, and value is the serialized block.
#[derive(Clone)]
pub struct BlockStore(sled::Tree);

impl BlockStore {
    /// Opens a new or existing `BlockStore` on the given sled database.
    pub fn new(db: &sled::Db, genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Result<Self> {
        let tree = db.open_tree(SLED_BLOCK_TREE)?;
        let store = Self(tree);
        // In case the store is empty, initialize it with the genesis block.
        if store.0.is_empty() {
            let genesis_block = Block::genesis_block(genesis_ts, genesis_data);
            store.insert(&[genesis_block])?;
        }

        Ok(store)
    }

    /// Insert a slice of [`Block`] into the store. With sled, the
    /// operation is done as a batch.
    /// The block's header is used as the key, while value is the serialized [`Block`] itself.
    pub fn insert(&self, blocks: &[Block]) -> Result<()> {
        let mut batch = sled::Batch::default();

        for block in blocks {
            batch.insert(block.header.as_bytes(), serialize(block));
        }

        self.0.apply_batch(batch)?;
        Ok(())
    }

    /// Check if the blockstore contains a given headerhash.
    pub fn contains(&self, headerhash: &blake3::Hash) -> Result<bool> {
        Ok(self.0.contains_key(headerhash.as_bytes())?)
    }

    /// Fetch given headerhashes from the blockstore.
    /// The resulting vector contains `Option`, which is `Some` if the block
    /// was found in the blockstore, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one block was not found.
    pub fn get(&self, headerhashes: &[blake3::Hash], strict: bool) -> Result<Vec<Option<Block>>> {
        let mut ret = Vec::with_capacity(headerhashes.len());

        for hash in headerhashes {
            if let Some(found) = self.0.get(hash.as_bytes())? {
                let block = deserialize(&found)?;
                ret.push(Some(block));
            } else {
                if strict {
                    let s = hash.to_hex().as_str().to_string();
                    return Err(Error::BlockNotFound(s))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Retrieve all blocks from the blockstore in the form of a tuple
    /// (`headerhash`, `block`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(blake3::Hash, Block)>> {
        let mut blocks = vec![];

        for block in self.0.iter() {
            let (key, value) = block.unwrap();
            let hash_bytes: [u8; 32] = key.as_ref().try_into().unwrap();
            let block = deserialize(&value)?;
            blocks.push((hash_bytes.into(), block));
        }

        Ok(blocks)
    }
}

/// The `BlockOrderStore` is a `sled` tree storing the order of the
/// blockchain's slots, where the key is the slot uid, and the value is
/// the block's headers' hash. [`BlockStore`] can be queried with this hash.
pub struct BlockOrderStore(sled::Tree);

impl BlockOrderStore {
    /// Opens a new or existing `BlockOrderStore` on the given sled database.
    pub fn new(db: &sled::Db, genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Result<Self> {
        let tree = db.open_tree(SLED_BLOCK_ORDER_TREE)?;
        let store = Self(tree);

        // In case the store is empty, initialize it with the genesis block.
        if store.0.is_empty() {
            let genesis_block = Block::genesis_block(genesis_ts, genesis_data);
            store.insert(&[0], &[genesis_block.header])?;
        }

        Ok(store)
    }

    /// Insert a slice of slots and headerhashes into the store. With sled, the
    /// operation is done as a batch.
    /// The block slot is used as the key, and the headerhash is used as value.
    pub fn insert(&self, slots: &[u64], hashes: &[blake3::Hash]) -> Result<()> {
        assert_eq!(slots.len(), hashes.len());
        let mut batch = sled::Batch::default();

        for (i, sl) in slots.iter().enumerate() {
            batch.insert(&sl.to_be_bytes(), hashes[i].as_bytes());
        }

        self.0.apply_batch(batch)?;
        Ok(())
    }

    /// Check if the blockorderstore contains a given slot.
    pub fn contains(&self, slot: u64) -> Result<bool> {
        Ok(self.0.contains_key(slot.to_be_bytes())?)
    }

    /// Fetch given slots from the blockorderstore.
    /// The resulting vector contains `Option`, which is `Some` if the slot
    /// was found in the blockstore, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one slot was not found.
    pub fn get(&self, slots: &[u64], strict: bool) -> Result<Vec<Option<blake3::Hash>>> {
        let mut ret = Vec::with_capacity(slots.len());

        for slot in slots {
            if let Some(found) = self.0.get(slot.to_be_bytes())? {
                let hash_bytes: [u8; 32] = found.as_ref().try_into().unwrap();
                let hash = blake3::Hash::from(hash_bytes);
                ret.push(Some(hash));
            } else {
                if strict {
                    return Err(Error::SlotNotFound(*slot))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Retrieve all slots from the blockorderstore in the form of a tuple
    /// (`slot`, `headerhash`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(u64, blake3::Hash)>> {
        let mut slots = vec![];

        for slot in self.0.iter() {
            let (key, value) = slot.unwrap();
            let slot_bytes: [u8; 8] = key.as_ref().try_into().unwrap();
            let hash_bytes: [u8; 32] = value.as_ref().try_into().unwrap();
            let slot = u64::from_be_bytes(slot_bytes);
            let hash = blake3::Hash::from(hash_bytes);
            slots.push((slot, hash));
        }

        Ok(slots)
    }

    /// Fetch n hashes after given slot. In the iteration, if a slot is not
    /// found, the iteration stops and the function returns what it has found
    /// so far in the `BlockOrderStore`.
    pub fn get_after(&self, slot: u64, n: u64) -> Result<Vec<blake3::Hash>> {
        let mut ret = vec![];

        let mut key = slot;
        let mut counter = 0;
        while counter <= n {
            if let Some(found) = self.0.get_gt(key.to_be_bytes())? {
                let key_bytes: [u8; 8] = found.0.as_ref().try_into().unwrap();
                key = u64::from_be_bytes(key_bytes);
                let header_hash = deserialize(&found.1)?;
                ret.push(header_hash);
                counter += 1;
                continue
            }
            break
        }

        Ok(ret)
    }

    /// Fetch the last block headerhash in the tree, based on the `Ord`
    /// implementation for `Vec<u8>`. This should not be able to
    /// fail because we initialize the store with the genesis block.
    pub fn get_last(&self) -> Result<(u64, blake3::Hash)> {
        let found = self.0.last()?.unwrap();

        let slot_bytes: [u8; 8] = found.0.as_ref().try_into().unwrap();
        let hash_bytes: [u8; 32] = found.1.as_ref().try_into().unwrap();
        let slot = u64::from_be_bytes(slot_bytes);
        let hash = blake3::Hash::from(hash_bytes);

        Ok((slot, hash))
    }
}
