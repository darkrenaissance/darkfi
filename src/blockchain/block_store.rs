/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use darkfi_sdk::{blockchain::Slot, crypto::schnorr::Signature};
use darkfi_serial::{deserialize, serialize, SerialDecodable, SerialEncodable};

use crate::{tx::Transaction, Error, Result};

use super::{parse_record, parse_u64_key_record, validate_slot, Header, SledDbOverlayPtr};

/// Block version number
pub const BLOCK_VERSION: u8 = 1;

/// Block magic bytes
const BLOCK_MAGIC_BYTES: [u8; 4] = [0x11, 0x6d, 0x75, 0x1f];

/// This struct represents a tuple of the form (`magic`, `header`, `txs`, `producer`, `slots`).
/// The header and transactions are stored as hashes, while slots are stored as integers,
/// serving as pointers to the actual data in the sled database.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Block {
    /// Block magic bytes
    pub magic: [u8; 4],
    /// Block header
    pub header: blake3::Hash,
    /// Trasaction hashes
    pub txs: Vec<blake3::Hash>,
    /// Block producer info
    pub producer: BlockProducer,
    /// Slots up until this block
    pub slots: Vec<u64>,
}

impl Block {
    pub fn new(
        header: blake3::Hash,
        txs: Vec<blake3::Hash>,
        producer: BlockProducer,
        slots: Vec<u64>,
    ) -> Self {
        let magic = BLOCK_MAGIC_BYTES;
        Self { magic, header, txs, producer, slots }
    }

    /// Calculate the block hash
    pub fn blockhash(&self) -> blake3::Hash {
        blake3::hash(&serialize(self))
    }
}

/// Structure representing full block data.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BlockInfo {
    /// Block magic bytes
    pub magic: [u8; 4],
    /// Block header data
    pub header: Header,
    /// Transactions payload
    pub txs: Vec<Transaction>,
    /// Block producer info
    pub producer: BlockProducer,
    /// Slots payload
    pub slots: Vec<Slot>,
}

impl Default for BlockInfo {
    /// Represents the genesis block on current timestamp
    fn default() -> Self {
        let magic = BLOCK_MAGIC_BYTES;
        Self {
            magic,
            header: Header::default(),
            txs: vec![],
            producer: BlockProducer::default(),
            slots: vec![Slot::default()],
        }
    }
}

impl BlockInfo {
    pub fn new(
        header: Header,
        txs: Vec<Transaction>,
        producer: BlockProducer,
        slots: Vec<Slot>,
    ) -> Self {
        let magic = BLOCK_MAGIC_BYTES;
        Self { magic, header, txs, producer, slots }
    }

    /// Calculate the block hash
    pub fn blockhash(&self) -> blake3::Hash {
        let block: Block = self.clone().into();
        block.blockhash()
    }

    /// A block is considered valid when the following rules apply:
    ///     1. Parent hash is equal to the hash of the previous block
    ///     2. Timestamp increments previous block timestamp
    ///     3. Slot increments previous block slot
    ///     4. Slots vector is not empty and all its slots are valid
    ///     5. Slot is the same as the slots vector last slot id
    /// Additional validity rules can be applied.
    pub fn validate(&self, previous: &Self, expected_reward: u64) -> Result<()> {
        let error = Err(Error::BlockIsInvalid(self.blockhash().to_string()));
        let previous_hash = previous.blockhash();

        // Check previous hash (1)
        if self.header.previous != previous_hash {
            return error
        }

        // Check timestamps are incremental (2)
        if self.header.timestamp <= previous.header.timestamp {
            return error
        }

        // Check slots are incremental (3)
        if self.header.slot <= previous.header.slot {
            return error
        }

        // Verify slots (4)
        if self.slots.is_empty() {
            return error
        }

        // Retrieve previous block last slot
        let mut previous_slot = previous.slots.last().unwrap();

        // Check if empty slots existed
        if self.slots.len() > 1 {
            // All slots exluding the last one must have reward value set to 0.
            // Slots must already be in correct order (sorted by id).
            for slot in &self.slots[..self.slots.len() - 1] {
                validate_slot(slot, previous_slot, &previous_hash, &previous.header.previous, 0)?;
                previous_slot = slot;
            }
        }

        validate_slot(
            self.slots.last().unwrap(),
            previous_slot,
            &previous_hash,
            &previous.header.previous,
            expected_reward,
        )?;

        // Check block slot is the last slot id (5)
        if self.slots.last().unwrap().id != self.header.slot {
            return error
        }

        Ok(())
    }
}

impl From<BlockInfo> for Block {
    fn from(block_info: BlockInfo) -> Self {
        let txs = block_info.txs.iter().map(|x| blake3::hash(&serialize(x))).collect();
        let slots = block_info.slots.iter().map(|x| x.id).collect();
        Self {
            magic: block_info.magic,
            header: block_info.header.headerhash(),
            txs,
            producer: block_info.producer,
            slots,
        }
    }
}

/// [`Block`] sled tree
const SLED_BLOCK_TREE: &[u8] = b"_blocks";

/// The `BlockStore` is a `sled` tree storing all the blockchain's blocks
/// where the key is the blocks' hash, and value is the serialized block.
#[derive(Clone)]
pub struct BlockStore(pub sled::Tree);

impl BlockStore {
    /// Opens a new or existing `BlockStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_BLOCK_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a slice of [`Block`] into the store.
    pub fn insert(&self, blocks: &[Block]) -> Result<Vec<blake3::Hash>> {
        let (batch, ret) = self.insert_batch(blocks)?;
        self.0.apply_batch(batch)?;
        Ok(ret)
    }

    /// Generate the sled batch corresponding to an insert, so caller
    /// can handle the write operation.
    /// The blocks are hashed with BLAKE3 and this block hash is used as
    /// the key, while value is the serialized [`Block`] itself.
    /// On success, the function returns the block hashes in the same order.
    pub fn insert_batch(&self, blocks: &[Block]) -> Result<(sled::Batch, Vec<blake3::Hash>)> {
        let mut ret = Vec::with_capacity(blocks.len());
        let mut batch = sled::Batch::default();

        for block in blocks {
            let serialized = serialize(block);
            let blockhash = blake3::hash(&serialized);
            batch.insert(blockhash.as_bytes(), serialized);
            ret.push(blockhash);
        }

        Ok((batch, ret))
    }

    /// Check if the block store contains a given block hash.
    pub fn contains(&self, blockhash: &blake3::Hash) -> Result<bool> {
        Ok(self.0.contains_key(blockhash.as_bytes())?)
    }

    /// Fetch given block hashes from the block store.
    /// The resulting vector contains `Option`, which is `Some` if the block
    /// was found in the block store, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one block was not found.
    pub fn get(&self, block_hashes: &[blake3::Hash], strict: bool) -> Result<Vec<Option<Block>>> {
        let mut ret = Vec::with_capacity(block_hashes.len());

        for hash in block_hashes {
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

    /// Retrieve all blocks from the block store in the form of a tuple
    /// (`hash`, `block`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(blake3::Hash, Block)>> {
        let mut blocks = vec![];

        for block in self.0.iter() {
            blocks.push(parse_record(block.unwrap())?);
        }

        Ok(blocks)
    }
}

/// Overlay structure over a [`BlockStore`] instance.
pub struct BlockStoreOverlay(SledDbOverlayPtr);

impl BlockStoreOverlay {
    pub fn new(overlay: &SledDbOverlayPtr) -> Result<Self> {
        overlay.lock().unwrap().open_tree(SLED_BLOCK_TREE)?;
        Ok(Self(overlay.clone()))
    }

    /// Insert a slice of [`Block`] into the overlay.
    /// The block are hashed with BLAKE3 and this block hash is used as
    /// the key, while value is the serialized [`Block`] itself.
    /// On success, the function returns the block hashes in the same order.
    pub fn insert(&self, blocks: &[Block]) -> Result<Vec<blake3::Hash>> {
        let mut ret = Vec::with_capacity(blocks.len());
        let mut lock = self.0.lock().unwrap();

        for block in blocks {
            let serialized = serialize(block);
            let blockhash = blake3::hash(&serialized);
            lock.insert(SLED_BLOCK_TREE, blockhash.as_bytes(), &serialized)?;
            ret.push(blockhash);
        }

        Ok(ret)
    }

    /// Fetch given block hashes from the overlay.
    /// The resulting vector contains `Option`, which is `Some` if the block
    /// was found in the overlay, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one block was not found.
    pub fn get(&self, block_hashes: &[blake3::Hash], strict: bool) -> Result<Vec<Option<Block>>> {
        let mut ret = Vec::with_capacity(block_hashes.len());
        let lock = self.0.lock().unwrap();

        for hash in block_hashes {
            if let Some(found) = lock.get(SLED_BLOCK_TREE, hash.as_bytes())? {
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
}

/// Auxiliary structure used to keep track of blocks order.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct BlockOrder {
    /// Order number
    pub number: u64,
    /// Block headerhash of that number
    pub block: blake3::Hash,
}

/// [`BlockOrder`] sled tree
const SLED_BLOCK_ORDER_TREE: &[u8] = b"_block_order";

/// The `BlockOrderStore` is a `sled` tree storing the order of the
/// blockchain's blocks, where the key is the order number, and the value is
/// the blocks' hash. [`BlockStore`] can be queried with this hash.
#[derive(Clone)]
pub struct BlockOrderStore(pub sled::Tree);

impl BlockOrderStore {
    /// Opens a new or existing `BlockOrderStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_BLOCK_ORDER_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a slice of `u64` and block hashes into the store.
    pub fn insert(&self, order: &[u64], hashes: &[blake3::Hash]) -> Result<()> {
        let batch = self.insert_batch(order, hashes)?;
        self.0.apply_batch(batch)?;
        Ok(())
    }

    /// Generate the sled batch corresponding to an insert, so caller
    /// can handle the write operation.
    /// The block order number is used as the key, and the block hash is used as value.
    pub fn insert_batch(&self, order: &[u64], hashes: &[blake3::Hash]) -> Result<sled::Batch> {
        if order.len() != hashes.len() {
            return Err(Error::InvalidInputLengths)
        }

        let mut batch = sled::Batch::default();

        for (i, number) in order.iter().enumerate() {
            batch.insert(&number.to_be_bytes(), hashes[i].as_bytes());
        }

        Ok(batch)
    }

    /// Check if the block order store contains a given order number.
    pub fn contains(&self, number: u64) -> Result<bool> {
        Ok(self.0.contains_key(number.to_be_bytes())?)
    }

    /// Fetch given order numbers from the block order store.
    /// The resulting vector contains `Option`, which is `Some` if the number
    /// was found in the block order store, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one order number was not found.
    pub fn get(&self, order: &[u64], strict: bool) -> Result<Vec<Option<blake3::Hash>>> {
        let mut ret = Vec::with_capacity(order.len());

        for number in order {
            if let Some(found) = self.0.get(number.to_be_bytes())? {
                let block_hash = deserialize(&found)?;
                ret.push(Some(block_hash));
            } else {
                if strict {
                    return Err(Error::BlockNumberNotFound(*number))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Retrieve complete order from the block order store in the form of
    /// a vector containing (`number`, `hash`) tuples.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(u64, blake3::Hash)>> {
        let mut order = vec![];

        for record in self.0.iter() {
            order.push(parse_u64_key_record(record.unwrap())?);
        }

        Ok(order)
    }

    /// Fetch n hashes after given order number. In the iteration, if an order
    /// number is not found, the iteration stops and the function returns what
    /// it has found so far in the `BlockOrderStore`.
    pub fn get_after(&self, number: u64, n: u64) -> Result<Vec<blake3::Hash>> {
        let mut ret = vec![];

        let mut key = number;
        let mut counter = 0;
        while counter <= n {
            if let Some(found) = self.0.get_gt(key.to_be_bytes())? {
                let (number, hash) = parse_u64_key_record(found)?;
                key = number;
                ret.push(hash);
                counter += 1;
                continue
            }
            break
        }

        Ok(ret)
    }

    /// Fetch the first block hash in the tree, based on the `Ord`
    /// implementation for `Vec<u8>`.
    pub fn get_first(&self) -> Result<(u64, blake3::Hash)> {
        let found = match self.0.first()? {
            Some(s) => s,
            None => return Err(Error::BlockNumberNotFound(0)),
        };
        let (number, hash) = parse_u64_key_record(found)?;

        Ok((number, hash))
    }

    /// Fetch the last block hash in the tree, based on the `Ord`
    /// implementation for `Vec<u8>`.
    pub fn get_last(&self) -> Result<(u64, blake3::Hash)> {
        let found = self.0.last()?.unwrap();
        let (number, hash) = parse_u64_key_record(found)?;

        Ok((number, hash))
    }

    /// Retrieve records count
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if sled contains any records
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Overlay structure over a [`BlockOrderStore`] instance.
pub struct BlockOrderStoreOverlay(SledDbOverlayPtr);

impl BlockOrderStoreOverlay {
    pub fn new(overlay: &SledDbOverlayPtr) -> Result<Self> {
        overlay.lock().unwrap().open_tree(SLED_BLOCK_ORDER_TREE)?;
        Ok(Self(overlay.clone()))
    }

    /// Insert a slice of `u64` and block hashes into the store. With sled, the
    /// operation is done as a batch.
    /// The block order number is used as the key, and the blockhash is used as value.
    pub fn insert(&self, order: &[u64], hashes: &[blake3::Hash]) -> Result<()> {
        if order.len() != hashes.len() {
            return Err(Error::InvalidInputLengths)
        }

        let mut lock = self.0.lock().unwrap();

        for (i, number) in order.iter().enumerate() {
            lock.insert(SLED_BLOCK_ORDER_TREE, &number.to_be_bytes(), hashes[i].as_bytes())?;
        }

        Ok(())
    }

    /// Fetch given order numbers from the overlay.
    /// The resulting vector contains `Option`, which is `Some` if the number
    /// was found in the overlay, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one number was not found.
    pub fn get(&self, order: &[u64], strict: bool) -> Result<Vec<Option<blake3::Hash>>> {
        let mut ret = Vec::with_capacity(order.len());
        let lock = self.0.lock().unwrap();

        for number in order {
            if let Some(found) = lock.get(SLED_BLOCK_ORDER_TREE, &number.to_be_bytes())? {
                let block_hash = deserialize(&found)?;
                ret.push(Some(block_hash));
            } else {
                if strict {
                    return Err(Error::BlockNumberNotFound(*number))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Fetch the last block hash in the overlay, based on the `Ord`
    /// implementation for `Vec<u8>`.
    pub fn get_last(&self) -> Result<(u64, blake3::Hash)> {
        let found = self.0.lock().unwrap().last(SLED_BLOCK_ORDER_TREE)?.unwrap();
        let (number, hash) = parse_u64_key_record(found)?;

        Ok((number, hash))
    }

    /// Check if overlay contains any records
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.0.lock().unwrap().is_empty(SLED_BLOCK_ORDER_TREE)?)
    }
}

/// This struct represents [`Block`] producer information.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BlockProducer {
    /// Block producer signature
    pub signature: Signature,
    /// Proposal transaction
    pub proposal: Transaction,
}

impl BlockProducer {
    pub fn new(signature: Signature, proposal: Transaction) -> Self {
        Self { signature, proposal }
    }
}

impl Default for BlockProducer {
    fn default() -> Self {
        let signature = Signature::dummy();
        let proposal = Transaction::default();
        Self { signature, proposal }
    }
}
