/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
};
#[cfg(feature = "async-serial")]
use darkfi_serial::async_trait;

use darkfi_serial::{deserialize, serialize, Encodable, SerialDecodable, SerialEncodable};
use num_bigint::BigUint;

use crate::{tx::Transaction, Error, Result};

use super::{parse_record, parse_u64_key_record, Header, SledDbOverlayPtr};

/// This struct represents a tuple of the form (`header`, `txs`, `signature`).
/// The header and transactions are stored as hashes, serving as pointers to the actual data
/// in the sled database.
/// NOTE: This struct fields are considered final, as it represents a blockchain block.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Block {
    /// Block header
    pub header: blake3::Hash,
    /// Trasaction hashes
    pub txs: Vec<blake3::Hash>,
    /// Block producer signature
    pub signature: Signature,
}

impl Block {
    pub fn new(header: blake3::Hash, txs: Vec<blake3::Hash>, signature: Signature) -> Self {
        Self { header, txs, signature }
    }

    /// A block's hash is the same as the hash of its header
    pub fn hash(&self) -> blake3::Hash {
        self.header
    }

    /// Generate a `Block` from a `BlockInfo`
    pub fn from_block_info(block_info: &BlockInfo) -> Result<Self> {
        let header = block_info.header.hash()?;
        let txs = block_info.txs.iter().map(|x| blake3::hash(&serialize(x))).collect();
        let signature = block_info.signature;
        Ok(Self { header, txs, signature })
    }
}

/// Structure representing full block data, acting as
/// a wrapper struct over `Block`, enabling us to include
/// more information that might be used in different block
/// version, without affecting the original struct.
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
    pub fn hash(&self) -> Result<blake3::Hash> {
        self.header.hash()
    }

    /// Compute the block's full hash
    pub fn full_hash(&self) -> Result<blake3::Hash> {
        let mut hasher = blake3::Hasher::new();
        self.encode(&mut hasher)?;
        Ok(hasher.finalize())
    }

    /// Append a transaction to the block. Also adds it to the Merkle tree.
    pub fn append_tx(&mut self, tx: Transaction) -> Result<()> {
        append_tx_to_merkle_tree(&mut self.header.tree, &tx)?;
        self.txs.push(tx);

        Ok(())
    }

    /// Append a vector of transactions to the block. Also adds them to the
    /// Merkle tree.
    pub fn append_txs(&mut self, txs: Vec<Transaction>) -> Result<()> {
        for tx in txs {
            self.append_tx(tx)?;
        }

        Ok(())
    }

    /// Sign block header using provided secret key
    // TODO: sign more stuff?
    pub fn sign(&mut self, secret_key: &SecretKey) -> Result<()> {
        self.signature = secret_key.sign(&self.hash()?.as_bytes()[..]);

        Ok(())
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
    /// The block's hash() function output is used as the key,
    /// while value is the serialized [`Block`] itself.
    /// On success, the function returns the block hashes in the same order.
    pub fn insert_batch(&self, blocks: &[Block]) -> Result<(sled::Batch, Vec<blake3::Hash>)> {
        let mut ret = Vec::with_capacity(blocks.len());
        let mut batch = sled::Batch::default();

        for block in blocks {
            let blockhash = block.hash();
            batch.insert(blockhash.as_bytes(), serialize(block));
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
    /// The block's hash() function output is used as the key,
    /// while value is the serialized [`Block`] itself.
    /// On success, the function returns the block hashes in the same order.
    pub fn insert(&self, blocks: &[Block]) -> Result<Vec<blake3::Hash>> {
        let mut ret = Vec::with_capacity(blocks.len());
        let mut lock = self.0.lock().unwrap();

        for block in blocks {
            let blockhash = block.hash();
            lock.insert(SLED_BLOCK_TREE, blockhash.as_bytes(), &serialize(block))?;
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
/// the blocks' hash. [`BlockOrderStore`] can be queried with this order number.
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
        let found = match self.0.lock().unwrap().last(SLED_BLOCK_ORDER_TREE)? {
            Some(b) => b,
            None => return Err(Error::BlockNumberNotFound(0)),
        };
        let (number, hash) = parse_u64_key_record(found)?;

        Ok((number, hash))
    }

    /// Check if overlay contains any records
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.0.lock().unwrap().is_empty(SLED_BLOCK_ORDER_TREE)?)
    }
}

/// Auxiliary structure used to keep track of block PoW difficulty information.
/// Note: we only need height cummulative difficulty, but we also keep its actual
/// difficulty, so we can verify the sequence and/or know specific block height
/// difficulty, if ever needed.
#[derive(Debug)]
pub struct BlockDifficulty {
    /// Block height number
    pub height: u64,
    /// Block creation timestamp
    pub timestamp: u64,
    /// Height difficulty
    pub difficulty: BigUint,
    /// Height cummulative difficulty (total + height difficulty)
    pub cummulative_difficulty: BigUint,
}

impl BlockDifficulty {
    pub fn new(
        height: u64,
        timestamp: u64,
        difficulty: BigUint,
        cummulative_difficulty: BigUint,
    ) -> Self {
        Self { height, timestamp, difficulty, cummulative_difficulty }
    }
}

// Note: Doing all the imports here as this might get obselete if
// we implemented Encodable/Decodable for num_bigint::BigUint.
impl darkfi_serial::Encodable for BlockDifficulty {
    fn encode<S: std::io::Write>(&self, mut s: S) -> std::io::Result<usize> {
        let mut len = 0;
        len += self.height.encode(&mut s)?;
        len += self.timestamp.encode(&mut s)?;
        len += self.difficulty.to_bytes_be().encode(&mut s)?;
        len += self.cummulative_difficulty.to_bytes_be().encode(&mut s)?;
        Ok(len)
    }
}

impl darkfi_serial::Decodable for BlockDifficulty {
    fn decode<D: std::io::Read>(mut d: D) -> std::io::Result<Self> {
        let height: u64 = darkfi_serial::Decodable::decode(&mut d)?;
        let timestamp: u64 = darkfi_serial::Decodable::decode(&mut d)?;
        let bytes: Vec<u8> = darkfi_serial::Decodable::decode(&mut d)?;
        let difficulty: BigUint = BigUint::from_bytes_be(&bytes);
        let bytes: Vec<u8> = darkfi_serial::Decodable::decode(&mut d)?;
        let cummulative_difficulty: BigUint = BigUint::from_bytes_be(&bytes);
        let ret = Self { height, timestamp, difficulty, cummulative_difficulty };
        Ok(ret)
    }
}

/// [`BlockDifficulty`] sled tree
const SLED_BLOCK_DIFFICULTY_TREE: &[u8] = b"_block_difficulty";

/// The `BlockDifficultyStore` is a `sled` tree storing the difficulty information
/// of the blockchain's blocks, where the key is the block height number, and the
/// value is the blocks' hash. [`BlockDifficultyStore`] can be queried with this
/// height number.
#[derive(Clone)]
pub struct BlockDifficultyStore(pub sled::Tree);

impl BlockDifficultyStore {
    /// Opens a new or existing `BlockDifficultyStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_BLOCK_DIFFICULTY_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a slice of [`BlockDifficulty`] into the store.
    pub fn insert(&self, block_difficulties: &[BlockDifficulty]) -> Result<()> {
        let batch = self.insert_batch(block_difficulties)?;
        self.0.apply_batch(batch)?;
        Ok(())
    }

    /// Generate the sled batch corresponding to an insert, so caller
    /// can handle the write operation.
    /// The block's height number is used as the key, while value is
    //  the serialized [`BlockDifficulty`] itself.
    pub fn insert_batch(&self, block_difficulties: &[BlockDifficulty]) -> Result<sled::Batch> {
        let mut batch = sled::Batch::default();

        for block_difficulty in block_difficulties {
            batch.insert(&block_difficulty.height.to_be_bytes(), serialize(block_difficulty));
        }

        Ok(batch)
    }

    /// Fetch given block height numbers from the block difficulties store.
    /// The resulting vector contains `Option`, which is `Some` if the block
    /// height number was found in the block difficulties store, and otherwise
    /// it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one block height number was not found.
    pub fn get(&self, heights: &[u64], strict: bool) -> Result<Vec<Option<BlockDifficulty>>> {
        let mut ret = Vec::with_capacity(heights.len());

        for height in heights {
            if let Some(found) = self.0.get(height.to_be_bytes())? {
                let block_difficulty = deserialize(&found)?;
                ret.push(Some(block_difficulty));
            } else {
                if strict {
                    return Err(Error::BlockDifficultyNotFound(*height))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Fetch the last N records from the block difficulties store, in order.
    pub fn get_last_n(&self, n: usize) -> Result<Vec<BlockDifficulty>> {
        // Build an iterator to retrieve last N records
        let records = self.0.iter().rev().take(n);
        // Since the iterator grabs in right -> left order,
        // we deserialize found records, and push them in reverse order
        let mut last_n = vec![];
        for record in records {
            last_n.insert(0, deserialize(&record?.1)?);
        }

        Ok(last_n)
    }

    /// Retrieve all blockdifficulties from the block difficulties store in
    /// the form of a vector containing (`height`, `difficulty`) tuples.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(u64, BlockDifficulty)>> {
        let mut block_difficulties = vec![];

        for record in self.0.iter() {
            block_difficulties.push(parse_u64_key_record(record.unwrap())?);
        }

        Ok(block_difficulties)
    }
}

/// Overlay structure over a [`BlockDifficultyStore`] instance.
pub struct BlockDifficultyStoreOverlay(SledDbOverlayPtr);

impl BlockDifficultyStoreOverlay {
    pub fn new(overlay: &SledDbOverlayPtr) -> Result<Self> {
        overlay.lock().unwrap().open_tree(SLED_BLOCK_DIFFICULTY_TREE)?;
        Ok(Self(overlay.clone()))
    }

    /// Insert a slice of [`BlockDifficulty`] into the overlay.
    pub fn insert(&self, block_difficulties: &[BlockDifficulty]) -> Result<()> {
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
}

/// Auxiliary function to append a transaction to a Merkle tree.
pub fn append_tx_to_merkle_tree(tree: &mut MerkleTree, tx: &Transaction) -> Result<()> {
    let mut buf = [0u8; 64];
    buf[..blake3::OUT_LEN].copy_from_slice(tx.hash()?.as_bytes());
    let leaf = pallas::Base::from_uniform_bytes(&buf);
    tree.append(leaf.into());
    Ok(())
}
