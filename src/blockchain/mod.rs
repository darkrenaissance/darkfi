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

use std::sync::{Arc, Mutex};

use log::debug;
use sled::Transactional;

use darkfi_sdk::blockchain::Slot;
use darkfi_serial::{deserialize, serialize, Decodable};

use crate::{tx::Transaction, Error, Result};

/// Block related definitions and storage implementations
pub mod block_store;
pub use block_store::{
    Block, BlockInfo, BlockOrderStore, BlockOrderStoreOverlay, BlockStore, BlockStoreOverlay,
};

/// Header definition and storage implementation
pub mod header_store;
pub use header_store::{Header, HeaderStore, HeaderStoreOverlay};

/// Slots storage implementation
pub mod slot_store;
pub use slot_store::{validate_slot, SlotStore, SlotStoreOverlay};

/// Transactions related storage implementations
pub mod tx_store;
pub use tx_store::{PendingTxOrderStore, PendingTxStore, TxStore, TxStoreOverlay};

/// Contracts and Wasm storage implementations
pub mod contract_store;
pub use contract_store::{
    ContractStateStore, ContractStateStoreOverlay, WasmStore, WasmStoreOverlay,
};

/// Structure holding all sled trees that define the concept of Blockchain.
#[derive(Clone)]
pub struct Blockchain {
    /// Main pointer to the sled db connection
    pub sled_db: sled::Db,
    /// Headers sled tree
    pub headers: HeaderStore,
    /// Blocks sled tree
    pub blocks: BlockStore,
    /// Block order sled tree
    pub order: BlockOrderStore,
    /// Slot sled tree
    pub slots: SlotStore,
    /// Transactions sled tree
    pub transactions: TxStore,
    /// Pending transactions sled tree
    pub pending_txs: PendingTxStore,
    /// Pending transactions order sled tree
    pub pending_txs_order: PendingTxOrderStore,
    /// Contract states
    pub contracts: ContractStateStore,
    /// Wasm bincodes
    pub wasm_bincode: WasmStore,
}

impl Blockchain {
    /// Instantiate a new `Blockchain` with the given `sled` database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let headers = HeaderStore::new(db)?;
        let blocks = BlockStore::new(db)?;
        let order = BlockOrderStore::new(db)?;
        let slots = SlotStore::new(db)?;
        let transactions = TxStore::new(db)?;
        let pending_txs = PendingTxStore::new(db)?;
        let pending_txs_order = PendingTxOrderStore::new(db)?;
        let contracts = ContractStateStore::new(db)?;
        let wasm_bincode = WasmStore::new(db)?;

        Ok(Self {
            sled_db: db.clone(),
            headers,
            blocks,
            order,
            slots,
            transactions,
            pending_txs,
            pending_txs_order,
            contracts,
            wasm_bincode,
        })
    }

    /// A blockchain is considered valid, when every block is valid,
    /// based on validate_block checks.
    /// Be careful as this will try to load everything in memory.
    pub fn validate(&self) -> Result<()> {
        // We use block order store here so we have all blocks in order
        let blocks = self.order.get_all()?;
        for (index, block) in blocks[1..].iter().enumerate() {
            let full_blocks = self.get_blocks_by_hash(&[blocks[index].1, block.1])?;
            full_blocks[1].validate(&full_blocks[0])?;
        }

        Ok(())
    }

    /// Insert a given [`BlockInfo`] into the blockchain database.
    /// This functions wraps all the logic of separating the block into specific
    /// data that can be fed into the different trees of the database.
    /// Upon success, the functions returns the block hash that
    /// were given and appended to the ledger.
    pub fn add_block(&self, block: &BlockInfo) -> Result<blake3::Hash> {
        let mut trees = vec![];
        let mut batches = vec![];

        // Store transactions
        let (txs_batch, _) = self.transactions.insert_batch(&block.txs)?;
        trees.push(self.transactions.0.clone());
        batches.push(txs_batch);

        // Store header
        let (headers_batch, _) = self.headers.insert_batch(&[block.header.clone()])?;
        trees.push(self.headers.0.clone());
        batches.push(headers_batch);

        // Store block
        let blk: Block = Block::from(block.clone());
        let (bocks_batch, block_hashes) = self.blocks.insert_batch(&[blk])?;
        let block_hash = block_hashes[0];
        trees.push(self.blocks.0.clone());
        batches.push(bocks_batch);

        // Store block order
        let blocks_order_batch = self.order.insert_batch(&[block.header.slot], &[block_hash])?;
        trees.push(self.order.0.clone());
        batches.push(blocks_order_batch);

        // Store slot checkpoints
        let slots_batch = self.slots.insert_batch(&block.slots)?;
        trees.push(self.slots.0.clone());
        batches.push(slots_batch);

        // Perform an atomic transaction over the trees and apply the batches.
        self.atomic_write(&trees, &batches)?;

        Ok(block_hash)
    }

    /// Check if the given [`BlockInfo`] is in the database and all trees.
    pub fn has_block(&self, block: &BlockInfo) -> Result<bool> {
        let blockhash = match self.order.get(&[block.header.slot], true) {
            Ok(v) => v[0].unwrap(),
            Err(_) => return Ok(false),
        };

        // Check if we have all transactions
        let txs: Vec<blake3::Hash> =
            block.txs.iter().map(|x| blake3::hash(&serialize(x))).collect();
        if self.transactions.get(&txs, true).is_err() {
            return Ok(false)
        }

        // Check if we have all slots
        let slots: Vec<u64> = block.slots.iter().map(|x| x.id).collect();
        if self.slots.get(&slots, true).is_err() {
            return Ok(false)
        }

        // Check provided info produces the same hash
        Ok(blockhash == block.blockhash())
    }

    /// Retrieve [`BlockInfo`]s by given hashes. Fails if any of them is not found.
    pub fn get_blocks_by_hash(&self, hashes: &[blake3::Hash]) -> Result<Vec<BlockInfo>> {
        let blocks = self.blocks.get(hashes, true)?;
        let blocks: Vec<Block> = blocks.iter().map(|x| x.clone().unwrap()).collect();
        let ret = self.get_blocks_infos(&blocks)?;

        Ok(ret)
    }

    /// Retrieve all [`BlockInfo`] for given slice of [`Block`].
    /// Fails if any of them is not found
    fn get_blocks_infos(&self, blocks: &[Block]) -> Result<Vec<BlockInfo>> {
        let mut ret = Vec::with_capacity(blocks.len());
        for block in blocks {
            let headers = self.headers.get(&[block.header], true)?;
            // Since we used strict get, its safe to unwrap here
            let header = headers[0].clone().unwrap();

            let txs = self.transactions.get(&block.txs, true)?;
            let txs = txs.iter().map(|x| x.clone().unwrap()).collect();

            let slots = self.slots.get(&block.slots, true)?;
            let slots = slots.iter().map(|x| x.clone().unwrap()).collect();

            let info = BlockInfo::new(header, txs, block.producer.clone(), slots);
            ret.push(info);
        }

        Ok(ret)
    }

    /// Retrieve [`BlockInfo`]s by given slots. Does not fail if any of them are not found.
    pub fn get_blocks_by_slot(&self, slots: &[u64]) -> Result<Vec<BlockInfo>> {
        debug!(target: "blockchain", "get_blocks_by_slot(): {:?}", slots);
        let blockhashes = self.order.get(slots, false)?;

        let mut hashes = vec![];
        for i in blockhashes.into_iter().flatten() {
            hashes.push(i);
        }

        self.get_blocks_by_hash(&hashes)
    }

    /// Retrieve n blocks after given start slot.
    pub fn get_blocks_after(&self, slot: u64, n: u64) -> Result<Vec<BlockInfo>> {
        debug!(target: "blockchain", "get_blocks_after(): {} -> {}", slot, n);
        let hashes = self.order.get_after(slot, n)?;
        self.get_blocks_by_hash(&hashes)
    }

    /// Retrieve stored blocks count
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Retrieve stored txs count
    pub fn txs_len(&self) -> usize {
        self.transactions.len()
    }

    /// Check if blockchain contains any blocks
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Retrieve genesis (first) block slot and hash.
    pub fn genesis(&self) -> Result<(u64, blake3::Hash)> {
        self.order.get_first()
    }

    /// Retrieve the last block slot and hash.
    pub fn last(&self) -> Result<(u64, blake3::Hash)> {
        self.order.get_last()
    }

    /// Retrieve the last block info.
    pub fn last_block(&self) -> Result<BlockInfo> {
        let (_, hash) = self.last()?;
        Ok(self.get_blocks_by_hash(&[hash])?[0].clone())
    }

    /// Retrieve the last slot.
    pub fn last_slot(&self) -> Result<Slot> {
        self.slots.get_last()
    }

    /// Retrieve n slots after given start slot.
    pub fn get_slots_after(&self, slot: u64, n: u64) -> Result<Vec<Slot>> {
        debug!(target: "blockchain", "get_slots_after(): {} -> {}", slot, n);
        self.slots.get_after(slot, n)
    }

    /// Retrieve [`Slot`]s by given ids. Does not fail if any of them are not found.
    pub fn get_slots_by_id(&self, ids: &[u64]) -> Result<Vec<Option<Slot>>> {
        debug!(target: "blockchain", "get_slots_by_id(): {:?}", ids);
        self.slots.get(ids, true)
    }

    /// Check if the given [`Slot`] is in the database and all trees.
    pub fn has_slot(&self, slot: &Slot) -> Result<bool> {
        Ok(self.slots.get(&[slot.id], true).is_ok())
    }

    /// Check if block order for the given slot is in the database.
    pub fn has_slot_order(&self, slot: u64) -> Result<bool> {
        let vec = match self.order.get(&[slot], true) {
            Ok(v) => v,
            Err(_) => return Ok(false),
        };
        Ok(!vec.is_empty())
    }

    /// Insert a given slice of pending transactions into the blockchain database.
    /// On success, the function returns the transaction hashes in the same order
    /// as the input transactions.
    pub fn add_pending_txs(&self, txs: &[Transaction]) -> Result<Vec<blake3::Hash>> {
        let (txs_batch, txs_hashes) = self.pending_txs.insert_batch(txs)?;
        let txs_order_batch = self.pending_txs_order.insert_batch(&txs_hashes)?;

        // Perform an atomic transaction over the trees and apply the batches.
        let trees = [self.pending_txs.0.clone(), self.pending_txs_order.0.clone()];
        let batches = [txs_batch, txs_order_batch];
        self.atomic_write(&trees, &batches)?;

        Ok(txs_hashes)
    }

    /// Retrieve all transactions from the pending tx store.
    /// Be careful as this will try to load everything in memory.
    pub fn get_pending_txs(&self) -> Result<Vec<Transaction>> {
        let txs = self.pending_txs.get_all()?;
        let indexes = self.pending_txs_order.get_all()?;
        if txs.len() != indexes.len() {
            return Err(Error::InvalidInputLengths)
        }

        let mut ret = Vec::with_capacity(txs.len());
        for index in indexes {
            ret.push(txs.get(&index.1).unwrap().clone());
        }

        Ok(ret)
    }

    /// Remove a given slice of pending transactions from the blockchain database.
    pub fn remove_pending_txs(&self, txs: &[Transaction]) -> Result<()> {
        let txs_hashes: Vec<blake3::Hash> =
            txs.iter().map(|x| blake3::hash(&serialize(x))).collect();
        let indexes = self.pending_txs_order.get_all()?;
        // We could do indexes.iter().map(|x| txs_hashes.contains(x.1)).collect.map(|x| x.0).collect but this is faster
        // since we don't do the second iteration
        let mut removed_indexes = vec![];
        for index in indexes {
            if txs_hashes.contains(&index.1) {
                removed_indexes.push(index.0);
            }
        }

        let txs_batch = self.pending_txs.remove_batch(&txs_hashes);
        let txs_order_batch = self.pending_txs_order.remove_batch(&removed_indexes);

        // Perform an atomic transaction over the trees and apply the batches.
        let trees = [self.pending_txs.0.clone(), self.pending_txs_order.0.clone()];
        let batches = [txs_batch, txs_order_batch];
        self.atomic_write(&trees, &batches)?;

        Ok(())
    }

    /// Auxiliary function to write to multiple trees completely atomic.
    fn atomic_write(&self, trees: &[sled::Tree], batches: &[sled::Batch]) -> Result<()> {
        if trees.len() != batches.len() {
            return Err(Error::InvalidInputLengths)
        }

        trees.transaction(|trees| {
            for (index, tree) in trees.iter().enumerate() {
                tree.apply_batch(&batches[index])?;
            }

            Ok::<(), sled::transaction::ConflictableTransactionError<sled::Error>>(())
        })?;

        Ok(())
    }
}

/// Atomic pointer to sled db overlay.
pub type SledDbOverlayPtr = Arc<Mutex<sled_overlay::SledDbOverlay>>;

/// Atomic pointer to blockchain overlay.
pub type BlockchainOverlayPtr = Arc<Mutex<BlockchainOverlay>>;

/// Overlay structure over a [`Blockchain`] instance.
pub struct BlockchainOverlay {
    /// Main [`sled_overlay::SledDbOverlay`] to the sled db connection
    pub overlay: SledDbOverlayPtr,
    /// Headers overlay
    pub headers: HeaderStoreOverlay,
    /// Blocks overlay
    pub blocks: BlockStoreOverlay,
    /// Block order overlay
    pub order: BlockOrderStoreOverlay,
    /// Slots overlay
    pub slots: SlotStoreOverlay,
    /// Transactions overlay
    pub transactions: TxStoreOverlay,
    /// Contract states overlay
    pub contracts: ContractStateStoreOverlay,
    /// Wasm bincodes overlay
    pub wasm_bincode: WasmStoreOverlay,
}

impl BlockchainOverlay {
    /// Instantiate a new `BlockchainOverlay` over the given [`Blockchain`] instance.
    pub fn new(blockchain: &Blockchain) -> Result<BlockchainOverlayPtr> {
        let overlay = Arc::new(Mutex::new(sled_overlay::SledDbOverlay::new(&blockchain.sled_db)));
        let headers = HeaderStoreOverlay::new(&overlay)?;
        let blocks = BlockStoreOverlay::new(&overlay)?;
        let order = BlockOrderStoreOverlay::new(&overlay)?;
        let slots = SlotStoreOverlay::new(&overlay)?;
        let transactions = TxStoreOverlay::new(&overlay)?;
        let contracts = ContractStateStoreOverlay::new(&overlay)?;
        let wasm_bincode = WasmStoreOverlay::new(&overlay)?;

        Ok(Arc::new(Mutex::new(Self {
            overlay,
            headers,
            blocks,
            order,
            slots,
            transactions,
            contracts,
            wasm_bincode,
        })))
    }

    /// Check if blockchain contains any blocks
    pub fn is_empty(&self) -> Result<bool> {
        self.order.is_empty()
    }

    /// Retrieve the last block slot and hash.
    pub fn last(&self) -> Result<(u64, blake3::Hash)> {
        self.order.get_last()
    }

    /// Retrieve the last block info.
    pub fn last_block(&self) -> Result<BlockInfo> {
        let (_, hash) = self.last()?;
        Ok(self.get_blocks_by_hash(&[hash])?[0].clone())
    }

    /// Insert a given [`BlockInfo`] into the overlay.
    /// This functions wraps all the logic of separating the block into specific
    /// data that can be fed into the different trees of the overlay.
    /// Upon success, the functions returns the block hash that
    /// were given and appended to the overlay.
    /// Since we are adding to the overlay, we don't need to exeucte
    /// the writes atomically.
    pub fn add_block(&self, block: &BlockInfo) -> Result<blake3::Hash> {
        // Store transactions
        self.transactions.insert(&block.txs)?;

        // Store header
        self.headers.insert(&[block.header.clone()])?;

        // Store block
        let blk: Block = Block::from(block.clone());
        let block_hash = self.blocks.insert(&[blk])?[0];

        // Store block order
        self.order.insert(&[block.header.slot], &[block_hash])?;

        // Store slot checkpoints
        self.slots.insert(&block.slots)?;

        Ok(block_hash)
    }

    /// Check if the given [`BlockInfo`] is in the database and all trees.
    pub fn has_block(&self, block: &BlockInfo) -> Result<bool> {
        let blockhash = match self.order.get(&[block.header.slot], true) {
            Ok(v) => v[0].unwrap(),
            Err(_) => return Ok(false),
        };

        // Check if we have all transactions
        let txs: Vec<blake3::Hash> =
            block.txs.iter().map(|x| blake3::hash(&serialize(x))).collect();
        if self.transactions.get(&txs, true).is_err() {
            return Ok(false)
        }

        // Check if we have all slots
        let slots: Vec<u64> = block.slots.iter().map(|x| x.id).collect();
        if self.slots.get(&slots, true).is_err() {
            return Ok(false)
        }

        // Check provided info produces the same hash
        Ok(blockhash == block.blockhash())
    }

    /// Retrieve [`BlockInfo`]s by given hashes. Fails if any of them is not found.
    pub fn get_blocks_by_hash(&self, hashes: &[blake3::Hash]) -> Result<Vec<BlockInfo>> {
        let blocks = self.blocks.get(hashes, true)?;
        let blocks: Vec<Block> = blocks.iter().map(|x| x.clone().unwrap()).collect();
        let ret = self.get_blocks_infos(&blocks)?;

        Ok(ret)
    }

    /// Retrieve all [`BlockInfo`] for given slice of [`Block`].
    /// Fails if any of them is not found
    fn get_blocks_infos(&self, blocks: &[Block]) -> Result<Vec<BlockInfo>> {
        let mut ret = Vec::with_capacity(blocks.len());
        for block in blocks {
            let headers = self.headers.get(&[block.header], true)?;
            // Since we used strict get, its safe to unwrap here
            let header = headers[0].clone().unwrap();

            let txs = self.transactions.get(&block.txs, true)?;
            let txs = txs.iter().map(|x| x.clone().unwrap()).collect();

            let slots = self.slots.get(&block.slots, true)?;
            let slots = slots.iter().map(|x| x.clone().unwrap()).collect();

            let info = BlockInfo::new(header, txs, block.producer.clone(), slots);
            ret.push(info);
        }

        Ok(ret)
    }

    /// Checkpoint overlay so we can revert to it, if needed.
    pub fn checkpoint(&self) {
        self.overlay.lock().unwrap().checkpoint();
    }

    /// Revert to current overlay checkpoint.
    pub fn revert_to_checkpoint(&self) -> Result<()> {
        self.overlay.lock().unwrap().revert_to_checkpoint()?;

        Ok(())
    }
}

/// Parse a sled record in the form of a tuple (`key`, `value`).
pub fn parse_record<T1: Decodable, T2: Decodable>(
    record: (sled::IVec, sled::IVec),
) -> Result<(T1, T2)> {
    let key = deserialize(&record.0)?;
    let value = deserialize(&record.1)?;

    Ok((key, value))
}
