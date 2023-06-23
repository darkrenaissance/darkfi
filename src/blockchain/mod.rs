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

use darkfi_serial::serialize;

use crate::{
    consensus::{Block, BlockInfo, SlotCheckpoint},
    tx::Transaction,
    util::time::Timestamp,
    Result,
};

pub mod block_store;
pub use block_store::{BlockOrderStore, BlockStore, HeaderStore};

pub mod slot_checkpoint_store;
pub use slot_checkpoint_store::{SlotCheckpointStore, SlotCheckpointStoreOverlay};

pub mod tx_store;
pub use tx_store::{PendingTxOrderStore, PendingTxStore, TxStore};

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
    /// Slot checkpoints sled tree
    pub slot_checkpoints: SlotCheckpointStore,
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
    pub fn new(db: &sled::Db, genesis_ts: Timestamp, genesis_data: blake3::Hash) -> Result<Self> {
        let headers = HeaderStore::new(db, genesis_ts, genesis_data)?;
        let blocks = BlockStore::new(db, genesis_ts, genesis_data)?;
        let order = BlockOrderStore::new(db, genesis_ts, genesis_data)?;
        let slot_checkpoints = SlotCheckpointStore::new(db, genesis_data)?;
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
            slot_checkpoints,
            transactions,
            pending_txs,
            pending_txs_order,
            contracts,
            wasm_bincode,
        })
    }

    /// Insert a given slice of [`BlockInfo`] into the blockchain database.
    /// This functions wraps all the logic of separating the block into specific
    /// data that can be fed into the different trees of the database.
    /// Upon success, the functions returns a vector of the block hashes that
    /// were given and appended to the ledger.
    pub fn add(&self, blocks: &[BlockInfo]) -> Result<Vec<blake3::Hash>> {
        let mut ret = Vec::with_capacity(blocks.len());

        // TODO: Make db writes here completely atomic
        for block in blocks {
            // Store transactions
            self.transactions.insert(&block.txs)?;

            // Store header
            self.headers.insert(&[block.header.clone()])?;

            // Store block
            let blk: Block = Block::from(block.clone());
            let blockhash = self.blocks.insert(&[blk])?;
            ret.push(blockhash[0]);

            // Store block order
            self.order.insert(&[block.header.slot], &[blockhash[0]])?;
        }

        Ok(ret)
    }

    /// Check if the given [`BlockInfo`] is in the database and all trees.
    pub fn has_block(&self, block: &BlockInfo) -> Result<bool> {
        let blockhash = match self.order.get(&[block.header.slot], true) {
            Ok(v) => v[0].unwrap(),
            Err(_) => return Ok(false),
        };

        // TODO: Check if we have all transactions

        // Check provided info produces the same hash
        Ok(blockhash == block.blockhash())
    }

    /// Retrieve [`BlockInfo`]s by given hashes. Fails if any of them are not found.
    pub fn get_blocks_by_hash(&self, hashes: &[blake3::Hash]) -> Result<Vec<BlockInfo>> {
        let mut ret = Vec::with_capacity(hashes.len());

        let blocks = self.blocks.get(hashes, true)?;

        for block in blocks {
            let block = block.unwrap();

            let headers = self.headers.get(&[block.header], true)?;
            // Since we used strict get, its safe to unwrap here
            let header = headers[0].clone().unwrap();

            let txs = self.transactions.get(&block.txs, true)?;
            let txs = txs.iter().map(|x| x.clone().unwrap()).collect();

            let info = BlockInfo::new(header, txs, block.lead_info.clone());
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
        self.order.len() == 0
    }

    /// Retrieve the last block slot and hash.
    pub fn last(&self) -> Result<(u64, blake3::Hash)> {
        self.order.get_last()
    }

    /// Retrieve the last slot checkpoint.
    pub fn last_slot_checkpoint(&self) -> Result<SlotCheckpoint> {
        self.slot_checkpoints.get_last()
    }

    /// Retrieve n checkpoints after given start slot.
    pub fn get_slot_checkpoints_after(&self, slot: u64, n: u64) -> Result<Vec<SlotCheckpoint>> {
        debug!(target: "blockchain", "get_slot_checkpoints_after(): {} -> {}", slot, n);
        self.slot_checkpoints.get_after(slot, n)
    }

    /// Insert a given slice of [`SlotCheckpoint`] into the blockchain database.
    pub fn add_slot_checkpoints(&self, slot_checkpoints: &[SlotCheckpoint]) -> Result<()> {
        self.slot_checkpoints.insert(slot_checkpoints)
    }

    /// Retrieve [`SlotCheckpoint`]s by given slots. Does not fail if any of them are not found.
    pub fn get_slot_checkpoints_by_slot(
        &self,
        slots: &[u64],
    ) -> Result<Vec<Option<SlotCheckpoint>>> {
        debug!(target: "blockchain", "get_slot_checkpoints_by_slot(): {:?}", slots);
        self.slot_checkpoints.get(slots, true)
    }

    /// Check if the given [`SlotCheckpoint`] is in the database and all trees.
    pub fn has_slot_checkpoint(&self, slot_checkpoint: &SlotCheckpoint) -> Result<bool> {
        Ok(self.slot_checkpoints.get(&[slot_checkpoint.slot], true).is_ok())
    }

    /// Check if block order for the given slot is in the database.
    pub fn has_slot(&self, slot: u64) -> Result<bool> {
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
        // TODO: Make db writes here completely atomic
        let txs_hashes = self.pending_txs.insert(txs)?;
        self.pending_txs_order.insert(&txs_hashes)?;

        Ok(txs_hashes)
    }

    /// Retrieve all transactions from the pending tx store.
    /// Be careful as this will try to load everything in memory.
    pub fn get_pending_txs(&self) -> Result<Vec<Transaction>> {
        let txs = self.pending_txs.get_all()?;
        let indexes = self.pending_txs_order.get_all()?;
        assert_eq!(txs.len(), indexes.len());

        let mut ret = Vec::with_capacity(txs.len());
        for index in indexes {
            ret.push(txs.get(&index.1).unwrap().clone());
        }

        Ok(ret)
    }

    /// Remove a given slice of pending transactions from the blockchain database.
    pub fn remove_pending_txs(&self, txs: &[Transaction]) -> Result<()> {
        let mut txs_hashes = Vec::with_capacity(txs.len());
        for tx in txs {
            let tx_hash = blake3::hash(&serialize(tx));
            txs_hashes.push(tx_hash);
        }

        let indexes = self.pending_txs_order.get_all()?;
        let mut removed_indexes = vec![];
        for index in indexes {
            if txs_hashes.contains(&index.1) {
                removed_indexes.push(index.0);
            }
        }

        // TODO: Make db writes here completely atomic
        self.pending_txs.remove(&txs_hashes)?;
        self.pending_txs_order.remove(&removed_indexes)?;

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
    /// Slot checkpoints overlay
    pub slot_checkpoints: SlotCheckpointStoreOverlay,
    /// Contract states overlay
    pub contracts: ContractStateStoreOverlay,
    /// Wasm bincodes overlay
    pub wasm_bincode: WasmStoreOverlay,
}

impl BlockchainOverlay {
    /// Instantiate a new `BlockchainOverlay` over the given [`Blockchain`] instance.
    pub fn new(blockchain: &Blockchain) -> Result<BlockchainOverlayPtr> {
        let overlay = Arc::new(Mutex::new(sled_overlay::SledDbOverlay::new(&blockchain.sled_db)));
        let slot_checkpoints = SlotCheckpointStoreOverlay::new(overlay.clone())?;
        let contracts = ContractStateStoreOverlay::new(overlay.clone())?;
        let wasm_bincode = WasmStoreOverlay::new(overlay.clone())?;

        Ok(Arc::new(Mutex::new(Self { overlay, slot_checkpoints, contracts, wasm_bincode })))
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
