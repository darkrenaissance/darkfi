use crate::{
    consensus::Block,
    util::{
        serial::{deserialize, serialize},
        time::Timestamp,
    },
    Error, Result,
};

const SLED_BLOCK_TREE: &[u8] = b"_blocks";
const SLED_BLOCK_ORDER_TREE: &[u8] = b"_block_order";

/// The `BlockStore` is a `sled` tree storing all the blockchain's blocks
/// where the key is the block's hash, and value is the serialized block.
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

    /// Insert a slice of [`Block`] into the blockstore. With sled, the
    /// operation is done as a batch.
    /// The blocks are hashed with BLAKE3 and this blockhash is used as
    /// the key, while value is the serialized [`Block`] itself.
    /// On success, the function returns the block hashes in the same order.
    pub fn insert(&self, blocks: &[Block]) -> Result<Vec<blake3::Hash>> {
        let mut ret = Vec::with_capacity(blocks.len());
        let mut batch = sled::Batch::default();

        for block in blocks {
            let serialized = serialize(block);
            let blockhash = blake3::hash(&serialized);
            batch.insert(blockhash.as_bytes(), serialized);
            ret.push(blockhash);
        }

        self.0.apply_batch(batch)?;
        Ok(ret)
    }

    /// Check if the blockstore contains a given blockhash.
    pub fn contains(&self, blockhash: &blake3::Hash) -> Result<bool> {
        Ok(self.0.contains_key(blockhash.as_bytes())?)
    }

    /// Fetch given blockhashes from the blockstore.
    /// The resulting vector contains `Option`, which is `Some` if the block
    /// was found in the blockstore, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one block was not found.
    pub fn get(&self, blockhashes: &[blake3::Hash], strict: bool) -> Result<Vec<Option<Block>>> {
        let mut ret = Vec::with_capacity(blockhashes.len());

        for hash in blockhashes {
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
    /// (`blockhash`, `block`).
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
/// the block's hash. [`BlockStore`] can be queried with this hash.
pub struct BlockOrderStore(sled::Tree);

impl BlockOrderStore {
    /// Opens a new or existing `BlockOrderStore` on the given sled database.
    pub fn new(db: &sled::Db, genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Result<Self> {
        let tree = db.open_tree(SLED_BLOCK_ORDER_TREE)?;
        let store = Self(tree);

        // In case the store is empty, initialize it with the genesis block.
        if store.0.is_empty() {
            let genesis_block = Block::genesis_block(genesis_ts, genesis_data);
            let blockhash = blake3::hash(&serialize(&genesis_block));
            store.insert(&[genesis_block.sl], &[blockhash])?;
        }

        Ok(store)
    }

    /// Insert a slice of slots and blockhashes into the store. With sled, the
    /// operation is done as a batch.
    /// The block slot is used as the key, and the blockhash is used as value.
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
    /// (`slot`, `blockhash`).
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
                let block_hash = deserialize(&found.1)?;
                ret.push(block_hash);
                counter += 1;
                continue
            }
            break
        }

        Ok(ret)
    }

    /// Fetch the last block hash in the tree, based on the `Ord`
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
