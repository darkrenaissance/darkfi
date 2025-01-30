/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use darkfi_sdk::{
    crypto::{
        schnorr::{SchnorrSecret, Signature},
        MerkleTree, SecretKey,
    },
    pasta::{group::ff::FromUniformBytes, pallas},
    tx::TransactionHash,
};
#[cfg(feature = "async-serial")]
use darkfi_serial::async_trait;
use darkfi_serial::{deserialize, serialize, SerialDecodable, SerialEncodable};
use num_bigint::BigUint;
use sled_overlay::{
    serial::{parse_record, parse_u32_key_record},
    sled, SledDbOverlayStateDiff,
};

use crate::{tx::Transaction, util::time::Timestamp, Error, Result};

use super::{Header, HeaderHash, SledDbOverlayPtr};

/// This struct represents a tuple of the form (`header`, `txs`, `signature`).
///
/// The header and transactions are stored as hashes, serving as pointers to the actual data
/// in the sled database.
/// NOTE: This struct fields are considered final, as it represents a blockchain block.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Block {
    /// Block header
    pub header: HeaderHash,
    /// Trasaction hashes
    pub txs: Vec<TransactionHash>,
    /// Block producer signature
    pub signature: Signature,
}

impl Block {
    pub fn new(header: HeaderHash, txs: Vec<TransactionHash>, signature: Signature) -> Self {
        Self { header, txs, signature }
    }

    /// A block's hash is the same as the hash of its header
    pub fn hash(&self) -> HeaderHash {
        self.header
    }

    /// Generate a `Block` from a `BlockInfo`
    pub fn from_block_info(block_info: &BlockInfo) -> Self {
        let header = block_info.header.hash();
        let txs = block_info.txs.iter().map(|tx| tx.hash()).collect();
        let signature = block_info.signature;
        Self { header, txs, signature }
    }
}

/// Structure representing full block data.
///
/// It acts as a wrapper struct over `Block`, enabling us
/// to include more information that might be used in different
/// block versions, without affecting the original struct.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BlockInfo {
    /// Block header data
    pub header: Header,
    /// Transactions payload
    pub txs: Vec<Transaction>,
    /// Block producer signature
    pub signature: Signature,
}

impl Default for BlockInfo {
    /// Represents the genesis block on current timestamp
    fn default() -> Self {
        Self {
            header: Header::default(),
            txs: vec![Transaction::default()],
            signature: Signature::dummy(),
        }
    }
}

impl BlockInfo {
    pub fn new(header: Header, txs: Vec<Transaction>, signature: Signature) -> Self {
        Self { header, txs, signature }
    }

    /// Generate an empty block for provided Header.
    /// Transactions and the producer signature must be added after.
    pub fn new_empty(header: Header) -> Self {
        let txs = vec![];
        let signature = Signature::dummy();
        Self { header, txs, signature }
    }

    /// A block's hash is the same as the hash of its header
    pub fn hash(&self) -> HeaderHash {
        self.header.hash()
    }

    /// Append a transaction to the block. Also adds it to the Merkle tree.
    /// Note: when we append a tx we rebuild the whole tree, so its preferable
    /// to append them all at once using `append_txs`.
    pub fn append_tx(&mut self, tx: Transaction) {
        let mut tree = MerkleTree::new(1);
        // Append existing block transactions to the tree
        for block_tx in &self.txs {
            append_tx_to_merkle_tree(&mut tree, block_tx);
        }
        // Append the new transaction
        append_tx_to_merkle_tree(&mut tree, &tx);
        self.txs.push(tx);
        // Grab the tree root and store it in the header
        self.header.root = tree.root(0).unwrap();
    }

    /// Append a vector of transactions to the block. Also adds them to the
    /// Merkle tree.
    /// Note: when we append txs we rebuild the whole tree, so its preferable
    /// to append them all at once.
    pub fn append_txs(&mut self, txs: Vec<Transaction>) {
        let mut tree = MerkleTree::new(1);
        // Append existing block transactions to the tree
        for block_tx in &self.txs {
            append_tx_to_merkle_tree(&mut tree, block_tx);
        }
        // Append the new transactions
        for tx in txs {
            append_tx_to_merkle_tree(&mut tree, &tx);
            self.txs.push(tx);
        }
        // Grab the tree root and store it in the header
        self.header.root = tree.root(0).unwrap();
    }

    /// Sign block header using provided secret key
    pub fn sign(&mut self, secret_key: &SecretKey) {
        self.signature = secret_key.sign(self.hash().inner());
    }
}

/// Auxiliary structure used to keep track of blocks order.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct BlockOrder {
    /// Block height
    pub height: u32,
    /// Block header hash of that height
    pub block: HeaderHash,
}

/// Auxiliary structure used to keep track of block ranking information.
///
/// Note: we only need height cummulative ranks, but we also keep its actual
/// ranks, so we can verify the sequence and/or know specific block height
/// ranks, if ever needed.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct BlockRanks {
    /// Block target rank
    pub target_rank: BigUint,
    /// Height cummulative targets rank
    pub targets_rank: BigUint,
    /// Block hash rank
    pub hash_rank: BigUint,
    /// Height cummulative hashes rank
    pub hashes_rank: BigUint,
}

impl BlockRanks {
    pub fn new(
        target_rank: BigUint,
        targets_rank: BigUint,
        hash_rank: BigUint,
        hashes_rank: BigUint,
    ) -> Self {
        Self { target_rank, targets_rank, hash_rank, hashes_rank }
    }
}

/// Auxiliary structure used to keep track of block PoW difficulty information.
///
/// Note: we only need height cummulative difficulty, but we also keep its actual
/// difficulty, so we can verify the sequence and/or know specific block height
/// difficulty, if ever needed.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct BlockDifficulty {
    /// Block height number
    pub height: u32,
    /// Block creation timestamp
    pub timestamp: Timestamp,
    /// Height difficulty
    pub difficulty: BigUint,
    /// Height cummulative difficulty (total + height difficulty)
    pub cummulative_difficulty: BigUint,
    /// Block ranks
    pub ranks: BlockRanks,
}

impl BlockDifficulty {
    pub fn new(
        height: u32,
        timestamp: Timestamp,
        difficulty: BigUint,
        cummulative_difficulty: BigUint,
        ranks: BlockRanks,
    ) -> Self {
        Self { height, timestamp, difficulty, cummulative_difficulty, ranks }
    }

    /// Represents the genesis block difficulty
    pub fn genesis(timestamp: Timestamp) -> Self {
        let ranks = BlockRanks::new(
            BigUint::from(0u64),
            BigUint::from(0u64),
            BigUint::from(0u64),
            BigUint::from(0u64),
        );
        BlockDifficulty::new(0u32, timestamp, BigUint::from(0u64), BigUint::from(0u64), ranks)
    }
}

pub const SLED_BLOCK_TREE: &[u8] = b"_blocks";
pub const SLED_BLOCK_ORDER_TREE: &[u8] = b"_block_order";
pub const SLED_BLOCK_DIFFICULTY_TREE: &[u8] = b"_block_difficulty";
pub const SLED_BLOCK_STATE_DIFF_TREE: &[u8] = b"_block_state_diff";

/// The `BlockStore` is a structure representing all `sled` trees related
/// to storing the blockchain's blocks information.
#[derive(Clone)]
pub struct BlockStore {
    /// Main `sled` tree, storing all the blockchain's blocks, where the
    /// key is the blocks' hash, and value is the serialized block.
    pub main: sled::Tree,
    /// The `sled` tree storing the order of the blockchain's blocks,
    /// where the key is the height number, and the value is the blocks'
    /// hash.
    pub order: sled::Tree,
    /// The `sled` tree storing the difficulty information of the
    /// blockchain's blocks, where the key is the block height number,
    /// and the value is the blocks' hash.
    pub difficulty: sled::Tree,
    /// The `sled` tree storing each blocks' full database state changes,
    /// where the key is the block height number, and the value is the
    /// serialized database diff.
    pub state_diff: sled::Tree,
}

impl BlockStore {
    /// Opens a new or existing `BlockStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let main = db.open_tree(SLED_BLOCK_TREE)?;
        let order = db.open_tree(SLED_BLOCK_ORDER_TREE)?;
        let difficulty = db.open_tree(SLED_BLOCK_DIFFICULTY_TREE)?;
        let state_diff = db.open_tree(SLED_BLOCK_STATE_DIFF_TREE)?;
        Ok(Self { main, order, difficulty, state_diff })
    }

    /// Insert a slice of [`Block`] into the store's main tree.
    pub fn insert(&self, blocks: &[Block]) -> Result<Vec<HeaderHash>> {
        let (batch, ret) = self.insert_batch(blocks);
        self.main.apply_batch(batch)?;
        Ok(ret)
    }

    /// Insert a slice of `u32` and block hashes into the store's
    /// order tree.
    pub fn insert_order(&self, heights: &[u32], hashes: &[HeaderHash]) -> Result<()> {
        let batch = self.insert_batch_order(heights, hashes);
        self.order.apply_batch(batch)?;
        Ok(())
    }

    /// Insert a slice of [`BlockDifficulty`] into the store's
    /// difficulty tree.
    pub fn insert_difficulty(&self, block_difficulties: &[BlockDifficulty]) -> Result<()> {
        let batch = self.insert_batch_difficulty(block_difficulties);
        self.difficulty.apply_batch(batch)?;
        Ok(())
    }

    /// Insert a slice of `u32` and block diffs into the store's
    /// database diffs tree.
    pub fn insert_state_diff(
        &self,
        heights: &[u32],
        diffs: &[SledDbOverlayStateDiff],
    ) -> Result<()> {
        let batch = self.insert_batch_state_diff(heights, diffs);
        self.state_diff.apply_batch(batch)?;
        Ok(())
    }

    /// Generate the sled batch corresponding to an insert to the main
    /// tree, so caller can handle the write operation.
    /// The block's hash() function output is used as the key,
    /// while value is the serialized [`Block`] itself.
    /// On success, the function returns the block hashes in the same order.
    pub fn insert_batch(&self, blocks: &[Block]) -> (sled::Batch, Vec<HeaderHash>) {
        let mut ret = Vec::with_capacity(blocks.len());
        let mut batch = sled::Batch::default();

        for block in blocks {
            let blockhash = block.hash();
            batch.insert(blockhash.inner(), serialize(block));
            ret.push(blockhash);
        }

        (batch, ret)
    }

    /// Generate the sled batch corresponding to an insert to the order
    /// tree, so caller can handle the write operation.
    /// The block height is used as the key, and the block hash is used as value.
    pub fn insert_batch_order(&self, heights: &[u32], hashes: &[HeaderHash]) -> sled::Batch {
        let mut batch = sled::Batch::default();

        for (i, height) in heights.iter().enumerate() {
            batch.insert(&height.to_be_bytes(), hashes[i].inner());
        }

        batch
    }

    /// Generate the sled batch corresponding to an insert to the difficulty
    /// tree, so caller can handle the write operation.
    /// The block's height number is used as the key, while value is
    //  the serialized [`BlockDifficulty`] itself.
    pub fn insert_batch_difficulty(&self, block_difficulties: &[BlockDifficulty]) -> sled::Batch {
        let mut batch = sled::Batch::default();

        for block_difficulty in block_difficulties {
            batch.insert(&block_difficulty.height.to_be_bytes(), serialize(block_difficulty));
        }

        batch
    }

    /// Generate the sled batch corresponding to an insert to the database diffs
    /// tree, so caller can handle the write operation.
    /// The block height is used as the key, and the serialized database diff is
    /// used as value.
    pub fn insert_batch_state_diff(
        &self,
        heights: &[u32],
        diffs: &[SledDbOverlayStateDiff],
    ) -> sled::Batch {
        let mut batch = sled::Batch::default();

        for (i, height) in heights.iter().enumerate() {
            batch.insert(&height.to_be_bytes(), serialize(&diffs[i]));
        }

        batch
    }

    /// Check if the store's main tree contains a given block hash.
    pub fn contains(&self, blockhash: &HeaderHash) -> Result<bool> {
        Ok(self.main.contains_key(blockhash.inner())?)
    }

    /// Check if the store's order tree contains a given height.
    pub fn contains_order(&self, height: u32) -> Result<bool> {
        Ok(self.order.contains_key(height.to_be_bytes())?)
    }

    /// Fetch given block hashes from the store's main tree.
    /// The resulting vector contains `Option`, which is `Some` if the block
    /// was found in the block store, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one block was not found.
    pub fn get(&self, block_hashes: &[HeaderHash], strict: bool) -> Result<Vec<Option<Block>>> {
        let mut ret = Vec::with_capacity(block_hashes.len());

        for hash in block_hashes {
            if let Some(found) = self.main.get(hash.inner())? {
                let block = deserialize(&found)?;
                ret.push(Some(block));
                continue
            }
            if strict {
                return Err(Error::BlockNotFound(hash.as_string()))
            }
            ret.push(None);
        }

        Ok(ret)
    }

    /// Fetch given heights from the store's order tree.
    /// The resulting vector contains `Option`, which is `Some` if the height
    /// was found in the block order store, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one height was not found.
    pub fn get_order(&self, heights: &[u32], strict: bool) -> Result<Vec<Option<HeaderHash>>> {
        let mut ret = Vec::with_capacity(heights.len());

        for height in heights {
            if let Some(found) = self.order.get(height.to_be_bytes())? {
                let block_hash = deserialize(&found)?;
                ret.push(Some(block_hash));
                continue
            }
            if strict {
                return Err(Error::BlockHeightNotFound(*height))
            }
            ret.push(None);
        }

        Ok(ret)
    }

    /// Fetch given block height numbers from the store's difficulty tree.
    /// The resulting vector contains `Option`, which is `Some` if the block
    /// height number was found in the block difficulties store, and otherwise
    /// it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one block height number was not found.
    pub fn get_difficulty(
        &self,
        heights: &[u32],
        strict: bool,
    ) -> Result<Vec<Option<BlockDifficulty>>> {
        let mut ret = Vec::with_capacity(heights.len());

        for height in heights {
            if let Some(found) = self.difficulty.get(height.to_be_bytes())? {
                let block_difficulty = deserialize(&found)?;
                ret.push(Some(block_difficulty));
                continue
            }
            if strict {
                return Err(Error::BlockDifficultyNotFound(*height))
            }
            ret.push(None);
        }

        Ok(ret)
    }

    /// Fetch given block height numbers from the store's state diffs tree.
    /// The resulting vector contains `Option`, which is `Some` if the block
    /// height number was found in the block database diffs store, and otherwise
    /// it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one block height number was not found.
    pub fn get_state_diff(
        &self,
        heights: &[u32],
        strict: bool,
    ) -> Result<Vec<Option<SledDbOverlayStateDiff>>> {
        let mut ret = Vec::with_capacity(heights.len());

        for height in heights {
            if let Some(found) = self.state_diff.get(height.to_be_bytes())? {
                let state_diff = deserialize(&found)?;
                ret.push(Some(state_diff));
                continue
            }
            if strict {
                return Err(Error::BlockStateDiffNotFound(*height))
            }
            ret.push(None);
        }

        Ok(ret)
    }

    /// Retrieve all blocks from the store's main tree in the form of a
    /// tuple (`hash`, `block`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(HeaderHash, Block)>> {
        let mut blocks = vec![];

        for block in self.main.iter() {
            blocks.push(parse_record(block.unwrap())?);
        }

        Ok(blocks)
    }

    /// Retrieve complete order from the store's order tree in the form
    /// of a vector containing (`height`, `hash`) tuples.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all_order(&self) -> Result<Vec<(u32, HeaderHash)>> {
        let mut order = vec![];

        for record in self.order.iter() {
            order.push(parse_u32_key_record(record.unwrap())?);
        }

        Ok(order)
    }

    /// Fetches the blocks within a specified range of height from the store's order tree
    /// returning a collection of block heights with their associated [`HeaderHash`]s.
    pub fn get_order_by_range(&self, start: u32, end: u32) -> Result<Vec<(u32, HeaderHash)>> {
        if start >= end {
            return Err(Error::DatabaseError(format!(
                "Heights range is invalid: {}..{}",
                start, end
            )))
        }

        let mut blocks = vec![];

        let start_key = start.to_be_bytes();
        let end_key = end.to_be_bytes();

        for block in self.order.range(start_key..end_key) {
            blocks.push(parse_u32_key_record(block.unwrap())?);
        }

        Ok(blocks)
    }

    /// Retrieve all block difficulties from the store's difficulty tree in
    /// the form of a vector containing (`height`, `difficulty`) tuples.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all_difficulty(&self) -> Result<Vec<(u32, BlockDifficulty)>> {
        let mut block_difficulties = vec![];

        for record in self.difficulty.iter() {
            block_difficulties.push(parse_u32_key_record(record.unwrap())?);
        }

        Ok(block_difficulties)
    }

    /// Fetch n hashes before given height. In the iteration, if an order
    /// height is not found, the iteration stops and the function returns what
    /// it has found so far in the store's order tree.
    pub fn get_before(&self, height: u32, n: usize) -> Result<Vec<HeaderHash>> {
        let mut ret = vec![];

        let mut key = height;
        let mut counter = 0;
        while counter < n {
            let record = self.order.get_lt(key.to_be_bytes())?;
            if record.is_none() {
                break
            }
            // Since the iterator grabs in right -> left order,
            // we deserialize found records, and push them in reverse order
            let (height, hash) = parse_u32_key_record(record.unwrap())?;
            key = height;
            ret.insert(0, hash);
            counter += 1;
        }

        Ok(ret)
    }

    /// Fetch all hashes after given height. In the iteration, if an order
    /// height is not found, the iteration stops and the function returns what
    /// it has found so far in the store's order tree.
    pub fn get_all_after(&self, height: u32) -> Result<Vec<HeaderHash>> {
        let mut ret = vec![];

        let mut key = height;
        while let Some(found) = self.order.get_gt(key.to_be_bytes())? {
            let (height, hash) = parse_u32_key_record(found)?;
            key = height;
            ret.push(hash);
        }

        Ok(ret)
    }

    /// Fetch the first block hash in the order tree, based on the `Ord`
    /// implementation for `Vec<u8>`.
    pub fn get_first(&self) -> Result<(u32, HeaderHash)> {
        let Some(found) = self.order.first()? else { return Err(Error::BlockHeightNotFound(0u32)) };
        let (height, hash) = parse_u32_key_record(found)?;

        Ok((height, hash))
    }

    /// Fetch the last block hash in the order tree, based on the `Ord`
    /// implementation for `Vec<u8>`.
    pub fn get_last(&self) -> Result<(u32, HeaderHash)> {
        let found = self.order.last()?.unwrap();
        let (height, hash) = parse_u32_key_record(found)?;

        Ok((height, hash))
    }

    /// Fetch the last N records from order tree
    pub fn get_last_n_orders(&self, n: usize) -> Result<Vec<(u32, HeaderHash)>> {
        // Build an iterator to retrieve last N records
        let records = self.order.iter().rev().take(n);

        // Since the iterator grabs in right -> left order,
        // we deserialize found records, and push them in reverse order
        let mut last_n = vec![];
        for record in records {
            let record = record?;
            let parsed_record = parse_u32_key_record(record)?;
            last_n.insert(0, parsed_record);
        }
        Ok(last_n)
    }

    /// Fetch the last record in the difficulty tree, based on the `Ord`
    /// implementation for `Vec<u8>`. If the tree is empty,
    /// returns `None`.
    pub fn get_last_difficulty(&self) -> Result<Option<BlockDifficulty>> {
        let Some(found) = self.difficulty.last()? else { return Ok(None) };
        let block_difficulty = deserialize(&found.1)?;
        Ok(Some(block_difficulty))
    }

    /// Fetch the last N records from the store's difficulty tree, in order.
    pub fn get_last_n_difficulties(&self, n: usize) -> Result<Vec<BlockDifficulty>> {
        // Build an iterator to retrieve last N records
        let records = self.difficulty.iter().rev().take(n);
        // Since the iterator grabs in right -> left order,
        // we deserialize found records, and push them in reverse order
        let mut last_n = vec![];
        for record in records {
            last_n.insert(0, deserialize(&record?.1)?);
        }

        Ok(last_n)
    }

    /// Fetch N records before given height from the store's difficulty tree, in order.
    /// In the iteration, if a record height is not found, the iteration stops and the
    /// function returns what it has found so far in the store's difficulty tree.
    pub fn get_difficulties_before(&self, height: u32, n: usize) -> Result<Vec<BlockDifficulty>> {
        let mut ret = vec![];

        let mut key = height;
        let mut counter = 0;
        while counter < n {
            let record = self.difficulty.get_lt(key.to_be_bytes())?;
            if record.is_none() {
                break
            }
            // Since the iterator grabs in right -> left order,
            // we deserialize found records, and push them in reverse order
            let (height, difficulty) = parse_u32_key_record(record.unwrap())?;
            key = height;
            ret.insert(0, difficulty);
            counter += 1;
        }

        Ok(ret)
    }

    /// Fetch all state diffs after given height. In the iteration, if a state
    /// diff is not found, the iteration stops and the function returns what
    /// it has found so far in the store's state diffs tree.
    pub fn get_state_diffs_after(&self, height: u32) -> Result<Vec<SledDbOverlayStateDiff>> {
        let mut ret = vec![];

        let mut key = height;
        while let Some(found) = self.state_diff.get_gt(key.to_be_bytes())? {
            let (height, state_diff) = parse_u32_key_record(found)?;
            key = height;
            ret.push(state_diff);
        }

        Ok(ret)
    }

    /// Retrieve store's order tree records count.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Check if store's order tree contains any records.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

/// Overlay structure over a [`BlockStore`] instance.
pub struct BlockStoreOverlay(SledDbOverlayPtr);

impl BlockStoreOverlay {
    pub fn new(overlay: &SledDbOverlayPtr) -> Result<Self> {
        overlay.lock().unwrap().open_tree(SLED_BLOCK_TREE, true)?;
        overlay.lock().unwrap().open_tree(SLED_BLOCK_ORDER_TREE, true)?;
        overlay.lock().unwrap().open_tree(SLED_BLOCK_DIFFICULTY_TREE, true)?;
        overlay.lock().unwrap().open_tree(SLED_BLOCK_STATE_DIFF_TREE, true)?;
        Ok(Self(overlay.clone()))
    }

    /// Insert a slice of [`Block`] into the overlay's main tree.
    /// The block's hash() function output is used as the key,
    /// while value is the serialized [`Block`] itself.
    /// On success, the function returns the block hashes in the same order.
    pub fn insert(&self, blocks: &[Block]) -> Result<Vec<HeaderHash>> {
        let mut ret = Vec::with_capacity(blocks.len());
        let mut lock = self.0.lock().unwrap();

        for block in blocks {
            let blockhash = block.hash();
            lock.insert(SLED_BLOCK_TREE, blockhash.inner(), &serialize(block))?;
            ret.push(blockhash);
        }

        Ok(ret)
    }

    /// Insert a slice of `u32` and block hashes into overlay's order tree.
    /// The block height is used as the key, and the blockhash is used as value.
    pub fn insert_order(&self, heights: &[u32], hashes: &[HeaderHash]) -> Result<()> {
        if heights.len() != hashes.len() {
            return Err(Error::InvalidInputLengths)
        }

        let mut lock = self.0.lock().unwrap();

        for (i, height) in heights.iter().enumerate() {
            lock.insert(SLED_BLOCK_ORDER_TREE, &height.to_be_bytes(), hashes[i].inner())?;
        }

        Ok(())
    }

    /// Insert a slice of [`BlockDifficulty`] into the overlay's difficulty tree.
    pub fn insert_difficulty(&self, block_difficulties: &[BlockDifficulty]) -> Result<()> {
        let mut lock = self.0.lock().unwrap();

        for block_difficulty in block_difficulties {
            lock.insert(
                SLED_BLOCK_DIFFICULTY_TREE,
                &block_difficulty.height.to_be_bytes(),
                &serialize(block_difficulty),
            )?;
        }

        Ok(())
    }

    /// Fetch given block hashes from the overlay's main tree.
    /// The resulting vector contains `Option`, which is `Some` if the block
    /// was found in the overlay, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one block was not found.
    pub fn get(&self, block_hashes: &[HeaderHash], strict: bool) -> Result<Vec<Option<Block>>> {
        let mut ret = Vec::with_capacity(block_hashes.len());
        let lock = self.0.lock().unwrap();

        for hash in block_hashes {
            if let Some(found) = lock.get(SLED_BLOCK_TREE, hash.inner())? {
                let block = deserialize(&found)?;
                ret.push(Some(block));
                continue
            }
            if strict {
                return Err(Error::BlockNotFound(hash.as_string()))
            }
            ret.push(None);
        }

        Ok(ret)
    }

    /// Fetch given heights from the overlay's order tree.
    /// The resulting vector contains `Option`, which is `Some` if the height
    /// was found in the overlay, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one height was not found.
    pub fn get_order(&self, heights: &[u32], strict: bool) -> Result<Vec<Option<HeaderHash>>> {
        let mut ret = Vec::with_capacity(heights.len());
        let lock = self.0.lock().unwrap();

        for height in heights {
            if let Some(found) = lock.get(SLED_BLOCK_ORDER_TREE, &height.to_be_bytes())? {
                let block_hash = deserialize(&found)?;
                ret.push(Some(block_hash));
                continue
            }
            if strict {
                return Err(Error::BlockHeightNotFound(*height))
            }
            ret.push(None);
        }

        Ok(ret)
    }

    /// Fetch the last block hash in the overlay's order tree, based on the `Ord`
    /// implementation for `Vec<u8>`.
    pub fn get_last(&self) -> Result<(u32, HeaderHash)> {
        let found = match self.0.lock().unwrap().last(SLED_BLOCK_ORDER_TREE)? {
            Some(b) => b,
            None => return Err(Error::BlockHeightNotFound(0u32)),
        };
        let (height, hash) = parse_u32_key_record(found)?;

        Ok((height, hash))
    }

    /// Check if overlay's order tree contains any records.
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.0.lock().unwrap().is_empty(SLED_BLOCK_ORDER_TREE)?)
    }
}

/// Auxiliary function to append a transaction to a Merkle tree.
pub fn append_tx_to_merkle_tree(tree: &mut MerkleTree, tx: &Transaction) {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(tx.hash().inner());
    let leaf = pallas::Base::from_uniform_bytes(&buf);
    tree.append(leaf.into());
}
