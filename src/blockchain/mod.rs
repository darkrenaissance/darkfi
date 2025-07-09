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

use std::sync::{Arc, Mutex};

use darkfi_sdk::{monotree::Monotree, tx::TransactionHash};
use sled_overlay::{sled, sled::Transactional};
use tracing::debug;

use crate::{tx::Transaction, util::time::Timestamp, Error, Result};

/// Block related definitions and storage implementations
pub mod block_store;
pub use block_store::{
    Block, BlockDifficulty, BlockInfo, BlockStore, BlockStoreOverlay, SLED_BLOCK_DIFFICULTY_TREE,
    SLED_BLOCK_ORDER_TREE, SLED_BLOCK_STATE_INVERSE_DIFF_TREE, SLED_BLOCK_TREE,
};

/// Header definition and storage implementation
pub mod header_store;
pub use header_store::{
    Header, HeaderHash, HeaderStore, HeaderStoreOverlay, SLED_HEADER_TREE, SLED_SYNC_HEADER_TREE,
};

/// Transactions related storage implementations
pub mod tx_store;
pub use tx_store::{
    TxStore, TxStoreOverlay, SLED_PENDING_TX_ORDER_TREE, SLED_PENDING_TX_TREE,
    SLED_TX_LOCATION_TREE, SLED_TX_TREE,
};

/// Contracts and Wasm storage implementations
pub mod contract_store;
pub use contract_store::{
    ContractStore, ContractStoreOverlay, SLED_BINCODE_TREE, SLED_CONTRACTS_TREE,
};

/// Monero definitions needed for merge mining
pub mod monero;

/// Structure holding all sled trees that define the concept of Blockchain.
#[derive(Clone)]
pub struct Blockchain {
    /// Main pointer to the sled db connection
    pub sled_db: sled::Db,
    /// Headers sled tree
    pub headers: HeaderStore,
    /// Blocks sled tree
    pub blocks: BlockStore,
    /// Transactions related sled trees
    pub transactions: TxStore,
    /// Contracts related sled trees
    pub contracts: ContractStore,
}

impl Blockchain {
    /// Instantiate a new `Blockchain` with the given `sled` database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let headers = HeaderStore::new(db)?;
        let blocks = BlockStore::new(db)?;
        let transactions = TxStore::new(db)?;
        let contracts = ContractStore::new(db)?;

        Ok(Self { sled_db: db.clone(), headers, blocks, transactions, contracts })
    }

    /// Insert a given [`BlockInfo`] into the blockchain database.
    /// This functions wraps all the logic of separating the block into specific
    /// data that can be fed into the different trees of the database.
    /// Upon success, the functions returns the block hash that
    /// were given and appended to the ledger.
    pub fn add_block(&self, block: &BlockInfo) -> Result<HeaderHash> {
        let mut trees = vec![];
        let mut batches = vec![];

        // Store header
        let (headers_batch, _) = self.headers.insert_batch(&[block.header.clone()]);
        trees.push(self.headers.main.clone());
        batches.push(headers_batch);

        // Store block
        let blk: Block = Block::from_block_info(block);
        let (bocks_batch, block_hashes) = self.blocks.insert_batch(&[blk]);
        let block_hash = block_hashes[0];
        let block_hash_vec = [block_hash];
        trees.push(self.blocks.main.clone());
        batches.push(bocks_batch);

        // Store block order
        let blocks_order_batch =
            self.blocks.insert_batch_order(&[block.header.height], &block_hash_vec);
        trees.push(self.blocks.order.clone());
        batches.push(blocks_order_batch);

        // Store transactions
        let (txs_batch, txs_hashes) = self.transactions.insert_batch(&block.txs);
        trees.push(self.transactions.main.clone());
        batches.push(txs_batch);

        // Store transactions_locations
        let txs_locations_batch =
            self.transactions.insert_batch_location(&txs_hashes, block.header.height);
        trees.push(self.transactions.location.clone());
        batches.push(txs_locations_batch);

        // Perform an atomic transaction over the trees and apply the batches.
        self.atomic_write(&trees, &batches)?;

        Ok(block_hash)
    }

    /// Check if the given [`BlockInfo`] is in the database and all trees.
    pub fn has_block(&self, block: &BlockInfo) -> Result<bool> {
        let blockhash = match self.blocks.get_order(&[block.header.height], true) {
            Ok(v) => v[0].unwrap(),
            Err(_) => return Ok(false),
        };

        // Check if we have all transactions
        let txs: Vec<TransactionHash> = block.txs.iter().map(|tx| tx.hash()).collect();
        if self.transactions.get(&txs, true).is_err() {
            return Ok(false)
        }

        // Check provided info produces the same hash
        Ok(blockhash == block.hash())
    }

    /// Retrieve [`BlockInfo`]s by given hashes. Fails if any of them is not found.
    pub fn get_blocks_by_hash(&self, hashes: &[HeaderHash]) -> Result<Vec<BlockInfo>> {
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

            let info = BlockInfo::new(header, txs, block.signature);
            ret.push(info);
        }

        Ok(ret)
    }

    /// Retrieve [`BlockInfo`]s by given heights. Does not fail if any of them are not found.
    pub fn get_blocks_by_heights(&self, heights: &[u32]) -> Result<Vec<BlockInfo>> {
        debug!(target: "blockchain", "get_blocks_by_heights(): {heights:?}");
        let blockhashes = self.blocks.get_order(heights, false)?;

        let mut hashes = vec![];
        for i in blockhashes.into_iter().flatten() {
            hashes.push(i);
        }

        self.get_blocks_by_hash(&hashes)
    }

    /// Retrieve n headers before given block height.
    pub fn get_headers_before(&self, height: u32, n: usize) -> Result<Vec<Header>> {
        debug!(target: "blockchain", "get_headers_before(): {height} -> {n}");
        let hashes = self.blocks.get_before(height, n)?;
        let headers = self.headers.get(&hashes, true)?;
        Ok(headers.iter().map(|h| h.clone().unwrap()).collect())
    }

    /// Retrieve stored blocks count
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Retrieve stored txs count
    pub fn txs_len(&self) -> usize {
        self.transactions.len()
    }

    /// Check if blockchain contains any blocks
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Retrieve genesis (first) block height and hash.
    pub fn genesis(&self) -> Result<(u32, HeaderHash)> {
        self.blocks.get_first()
    }

    /// Retrieve genesis (first) block info.
    pub fn genesis_block(&self) -> Result<BlockInfo> {
        let (_, hash) = self.genesis()?;
        Ok(self.get_blocks_by_hash(&[hash])?[0].clone())
    }

    /// Retrieve the last block height and hash.
    pub fn last(&self) -> Result<(u32, HeaderHash)> {
        self.blocks.get_last()
    }

    /// Retrieve the last block header.
    pub fn last_header(&self) -> Result<Header> {
        let (_, hash) = self.last()?;
        Ok(self.headers.get(&[hash], true)?[0].clone().unwrap())
    }

    /// Retrieve the last block info.
    pub fn last_block(&self) -> Result<BlockInfo> {
        let (_, hash) = self.last()?;
        Ok(self.get_blocks_by_hash(&[hash])?[0].clone())
    }

    /// Retrieve the last block difficulty. If the tree is empty,
    /// returns `BlockDifficulty::genesis` difficulty.
    pub fn last_block_difficulty(&self) -> Result<BlockDifficulty> {
        if let Some(found) = self.blocks.get_last_difficulty()? {
            return Ok(found)
        }

        let genesis_block = self.genesis_block()?;
        Ok(BlockDifficulty::genesis(genesis_block.header.timestamp))
    }

    /// Check if block order for the given height is in the database.
    pub fn has_height(&self, height: u32) -> Result<bool> {
        let vec = match self.blocks.get_order(&[height], true) {
            Ok(v) => v,
            Err(_) => return Ok(false),
        };
        Ok(!vec.is_empty())
    }

    /// Insert a given slice of pending transactions into the blockchain database.
    /// On success, the function returns the transaction hashes in the same order
    /// as the input transactions.
    pub fn add_pending_txs(&self, txs: &[Transaction]) -> Result<Vec<TransactionHash>> {
        let (txs_batch, txs_hashes) = self.transactions.insert_batch_pending(txs);
        let txs_order_batch = self.transactions.insert_batch_pending_order(&txs_hashes)?;

        // Perform an atomic transaction over the trees and apply the batches.
        let trees = [self.transactions.pending.clone(), self.transactions.pending_order.clone()];
        let batches = [txs_batch, txs_order_batch];
        self.atomic_write(&trees, &batches)?;

        Ok(txs_hashes)
    }

    /// Retrieve all transactions from the pending tx store.
    /// Be careful as this will try to load everything in memory.
    pub fn get_pending_txs(&self) -> Result<Vec<Transaction>> {
        let txs = self.transactions.get_all_pending()?;
        let indexes = self.transactions.get_all_pending_order()?;
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
        let txs_hashes: Vec<TransactionHash> = txs.iter().map(|tx| tx.hash()).collect();
        self.remove_pending_txs_hashes(&txs_hashes)
    }

    /// Remove a given slice of pending transactions hashes from the blockchain database.
    pub fn remove_pending_txs_hashes(&self, txs: &[TransactionHash]) -> Result<()> {
        let indexes = self.transactions.get_all_pending_order()?;
        // We could do indexes.iter().map(|x| txs.contains(x.1)).collect.map(|x| x.0).collect
        // but this is faster since we don't do the second iteration
        let mut removed_indexes = vec![];
        for index in indexes {
            if txs.contains(&index.1) {
                removed_indexes.push(index.0);
            }
        }

        let txs_batch = self.transactions.remove_batch_pending(txs);
        let txs_order_batch = self.transactions.remove_batch_pending_order(&removed_indexes);

        // Perform an atomic transaction over the trees and apply the batches.
        let trees = [self.transactions.pending.clone(), self.transactions.pending_order.clone()];
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

    /// Retrieve all blocks contained in the blockchain in order.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<BlockInfo>> {
        let order = self.blocks.get_all_order()?;
        let order: Vec<HeaderHash> = order.iter().map(|x| x.1).collect();
        let blocks = self.get_blocks_by_hash(&order)?;

        Ok(blocks)
    }

    /// Retrieve [`BlockInfo`]s by given heights range.
    pub fn get_by_range(&self, start: u32, end: u32) -> Result<Vec<BlockInfo>> {
        let blockhashes = self.blocks.get_order_by_range(start, end)?;
        let hashes: Vec<HeaderHash> = blockhashes.into_iter().map(|(_, hash)| hash).collect();
        self.get_blocks_by_hash(&hashes)
    }

    /// Retrieve last 'N' [`BlockInfo`]s from the blockchain.
    pub fn get_last_n(&self, n: usize) -> Result<Vec<BlockInfo>> {
        let records = self.blocks.get_last_n_orders(n)?;

        let mut last_n = vec![];
        for record in records {
            let header_hash = record.1;
            let blocks = self.get_blocks_by_hash(&[header_hash])?;
            for block in blocks {
                last_n.push(block.clone());
            }
        }

        Ok(last_n)
    }

    /// Auxiliary function to reset the blockchain and consensus state
    /// to the provided block height.
    pub fn reset_to_height(&self, height: u32) -> Result<()> {
        // First we grab the last block height
        let (last, _) = self.last()?;

        // Check if request height is after our last height
        if height >= last {
            return Ok(())
        }

        // Grab all state inverse diffs until requested height,
        // going backwards.
        let heights: Vec<u32> = (height + 1..=last).rev().collect();
        let inverse_diffs = self.blocks.get_state_inverse_diff(&heights, true)?;

        // Create an overlay to apply the reverse diffs
        let overlay = BlockchainOverlay::new(self)?;

        // Apply the inverse diffs sequence
        let overlay_lock = overlay.lock().unwrap();
        let mut lock = overlay_lock.overlay.lock().unwrap();
        for inverse_diff in inverse_diffs {
            // Since we used strict retrieval it's safe to unwrap here
            let inverse_diff = inverse_diff.unwrap();
            lock.add_diff(&inverse_diff)?;
            lock.apply_diff(&inverse_diff)?;
            self.sled_db.flush()?;
        }
        drop(lock);
        drop(overlay_lock);

        Ok(())
    }

    /// Generate a Monotree(SMT) containing all contracts states
    /// checksums, along with the wasm bincodes checksum.
    ///
    /// Note: native contracts zkas tree and wasm bincodes are excluded.
    pub fn get_state_monotree(&self) -> Result<Monotree> {
        self.contracts.get_state_monotree(&self.sled_db)
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
    /// Transactions overlay
    pub transactions: TxStoreOverlay,
    /// Contract overlay
    pub contracts: ContractStoreOverlay,
}

impl BlockchainOverlay {
    /// Instantiate a new `BlockchainOverlay` over the given [`Blockchain`] instance.
    pub fn new(blockchain: &Blockchain) -> Result<BlockchainOverlayPtr> {
        // Here we configure all our blockchain sled trees to be protected in the overlay
        let protected_trees = vec![
            SLED_BLOCK_TREE,
            SLED_BLOCK_ORDER_TREE,
            SLED_BLOCK_DIFFICULTY_TREE,
            SLED_BLOCK_STATE_INVERSE_DIFF_TREE,
            SLED_HEADER_TREE,
            SLED_SYNC_HEADER_TREE,
            SLED_TX_TREE,
            SLED_TX_LOCATION_TREE,
            SLED_PENDING_TX_TREE,
            SLED_PENDING_TX_ORDER_TREE,
            SLED_CONTRACTS_TREE,
            SLED_BINCODE_TREE,
        ];
        let overlay = Arc::new(Mutex::new(sled_overlay::SledDbOverlay::new(
            &blockchain.sled_db,
            protected_trees,
        )));
        let headers = HeaderStoreOverlay::new(&overlay)?;
        let blocks = BlockStoreOverlay::new(&overlay)?;
        let transactions = TxStoreOverlay::new(&overlay)?;
        let contracts = ContractStoreOverlay::new(&overlay)?;

        Ok(Arc::new(Mutex::new(Self { overlay, headers, blocks, transactions, contracts })))
    }

    /// Check if blockchain contains any blocks
    pub fn is_empty(&self) -> Result<bool> {
        self.blocks.is_empty()
    }

    /// Retrieve the last block height and hash.
    pub fn last(&self) -> Result<(u32, HeaderHash)> {
        self.blocks.get_last()
    }

    /// Retrieve the last block info.
    pub fn last_block(&self) -> Result<BlockInfo> {
        let (_, hash) = self.last()?;
        Ok(self.get_blocks_by_hash(&[hash])?[0].clone())
    }

    /// Retrieve the last block height.
    pub fn last_block_height(&self) -> Result<u32> {
        Ok(self.last()?.0)
    }

    /// Retrieve the last block timestamp.
    pub fn last_block_timestamp(&self) -> Result<Timestamp> {
        let (_, hash) = self.last()?;
        Ok(self.get_blocks_by_hash(&[hash])?[0].header.timestamp)
    }

    /// Insert a given [`BlockInfo`] into the overlay.
    /// This functions wraps all the logic of separating the block into specific
    /// data that can be fed into the different trees of the overlay.
    /// Upon success, the functions returns the block hash that
    /// were given and appended to the overlay.
    /// Since we are adding to the overlay, we don't need to exeucte
    /// the writes atomically.
    pub fn add_block(&self, block: &BlockInfo) -> Result<HeaderHash> {
        // Store header
        self.headers.insert(&[block.header.clone()])?;

        // Store block
        let blk: Block = Block::from_block_info(block);
        let txs_hashes = blk.txs.clone();
        let block_hash = self.blocks.insert(&[blk])?[0];
        let block_hash_vec = [block_hash];

        // Store block order
        self.blocks.insert_order(&[block.header.height], &block_hash_vec)?;

        // Store transactions
        self.transactions.insert(&block.txs)?;

        // Store transactions locations
        self.transactions.insert_location(&txs_hashes, block.header.height)?;

        Ok(block_hash)
    }

    /// Check if the given [`BlockInfo`] is in the database and all trees.
    pub fn has_block(&self, block: &BlockInfo) -> Result<bool> {
        let blockhash = match self.blocks.get_order(&[block.header.height], true) {
            Ok(v) => v[0].unwrap(),
            Err(_) => return Ok(false),
        };

        // Check if we have all transactions
        let txs: Vec<TransactionHash> = block.txs.iter().map(|tx| tx.hash()).collect();
        if self.transactions.get(&txs, true).is_err() {
            return Ok(false)
        }

        // Check provided info produces the same hash
        Ok(blockhash == block.hash())
    }

    /// Retrieve [`Header`]s by given hashes. Fails if any of them is not found.
    pub fn get_headers_by_hash(&self, hashes: &[HeaderHash]) -> Result<Vec<Header>> {
        let headers = self.headers.get(hashes, true)?;
        let ret: Vec<Header> = headers.iter().map(|x| x.clone().unwrap()).collect();

        Ok(ret)
    }

    /// Retrieve [`BlockInfo`]s by given hashes. Fails if any of them is not found.
    pub fn get_blocks_by_hash(&self, hashes: &[HeaderHash]) -> Result<Vec<BlockInfo>> {
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

            let info = BlockInfo::new(header, txs, block.signature);
            ret.push(info);
        }

        Ok(ret)
    }

    /// Retrieve [`Block`]s by given hashes and return their transactions hashes.
    pub fn get_blocks_txs_hashes(&self, hashes: &[HeaderHash]) -> Result<Vec<TransactionHash>> {
        let blocks = self.blocks.get(hashes, true)?;
        let mut ret = vec![];
        for block in blocks {
            ret.extend_from_slice(&block.unwrap().txs);
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

    /// Auxiliary function to create a full clone using SledDbOverlay::clone,
    /// generating new pointers for the underlying overlays.
    pub fn full_clone(&self) -> Result<BlockchainOverlayPtr> {
        let overlay = Arc::new(Mutex::new(self.overlay.lock().unwrap().clone()));
        let headers = HeaderStoreOverlay::new(&overlay)?;
        let blocks = BlockStoreOverlay::new(&overlay)?;
        let transactions = TxStoreOverlay::new(&overlay)?;
        let contracts = ContractStoreOverlay::new(&overlay)?;

        Ok(Arc::new(Mutex::new(Self { overlay, headers, blocks, transactions, contracts })))
    }

    /// Generate a Monotree(SMT) containing all contracts states
    /// checksums, along with the wasm bincodes checksum.
    /// A clone is used so we are not affected by the opened trees
    /// during checksum computing.
    ///
    /// Note: native contracts zkas tree and wasm bincodes are excluded.
    pub fn get_state_monotree(&self) -> Result<Monotree> {
        self.full_clone()?.lock().unwrap().contracts.get_state_monotree()
    }
}
