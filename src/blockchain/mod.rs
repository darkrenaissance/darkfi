/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use darkfi_serial::serialize;
use log::debug;

use crate::{
    consensus::{Block, BlockInfo, SlotCheckpoint},
    util::time::Timestamp,
    Error, Result,
};

pub mod blockstore;
pub use blockstore::{BlockOrderStore, BlockStore, HeaderStore};

pub mod slotcheckpointstore;
pub use slotcheckpointstore::SlotCheckpointStore;

pub mod nfstore;
pub use nfstore::NullifierStore;

pub mod rootstore;
pub use rootstore::RootStore;

pub mod txstore;
pub use txstore::TxStore;

pub mod contractstore;
pub use contractstore::{ContractStateStore, WasmStore};

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
    /// Nullifiers sled tree
    pub nullifiers: NullifierStore,
    /// Merkle roots sled tree
    pub merkle_roots: RootStore,
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
        let slot_checkpoints = SlotCheckpointStore::new(db)?;
        let transactions = TxStore::new(db)?;
        let nullifiers = NullifierStore::new(db)?;
        let merkle_roots = RootStore::new(db)?;
        let contracts = ContractStateStore::new(db)?;
        let wasm_bincode = WasmStore::new(db)?;

        Ok(Self {
            sled_db: db.clone(),
            headers,
            blocks,
            order,
            slot_checkpoints,
            transactions,
            nullifiers,
            merkle_roots,
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
        debug!("get_blocks_by_slot(): {:?}", slots);
        let blockhashes = self.order.get(slots, false)?;

        let mut hashes = vec![];
        for i in blockhashes.into_iter().flatten() {
            hashes.push(i);
        }

        self.get_blocks_by_hash(&hashes)
    }

    /// Retrieve n blocks after given start slot.
    pub fn get_blocks_after(&self, slot: u64, n: u64) -> Result<Vec<BlockInfo>> {
        debug!("get_blocks_after(): {} -> {}", slot, n);
        let hashes = self.order.get_after(slot, n)?;
        self.get_blocks_by_hash(&hashes)
    }

    /// Retrieve stored blocks count
    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.len() == 0
    }

    /// Retrieve the last block slot and hash.
    pub fn last(&self) -> Result<(u64, blake3::Hash)> {
        self.order.get_last()
    }

    /// Retrieve last finalized block leader proof hash.
    pub fn get_last_proof_hash(&self) -> Result<blake3::Hash> {
        let (_, hash) = self.last().unwrap();
        let blocks = self.blocks.get(&[hash], true)?;
        // Since we used strict get, its safe to unwrap here
        let block = blocks[0].clone().unwrap();
        let hash = blake3::hash(&serialize(&block.lead_info.proof));
        Ok(hash)
    }

    pub fn get_proof_hash_by_slot(&self, slot: u64) -> Result<blake3::Hash> {
        let blocks = self.get_blocks_by_slot(&[slot]).unwrap();
        if blocks.is_empty() {
            return Err(Error::BlockNotFound("block not found".to_string()))
        }
        // Since we used strict get, its safe to unwrap here
        let block = blocks[0].clone();
        let hash = blake3::hash(&serialize(&block.lead_info.proof));
        Ok(hash)
    }

    /// Retrieve last finalized block slot offset
    pub fn get_last_offset(&self) -> Result<(u64, u64)> {
        let (slot, hash) = self.last().unwrap();
        let blocks = self.blocks.get(&[hash], true)?;
        // Since we used strict get, its safe to unwrap here
        let block = blocks[0].clone().unwrap();
        Ok((slot, block.lead_info.offset))
    }

    /// Retrieve the last slot checkpoint.
    pub fn last_slot_checkpoint(&self) -> Result<SlotCheckpoint> {
        self.slot_checkpoints.get_last()
    }

    /// Retrieve n checkpoints after given start slot.
    pub fn get_slot_checkpoints_after(&self, slot: u64, n: u64) -> Result<Vec<SlotCheckpoint>> {
        debug!("get_slot_checkpoints_after(): {} -> {}", slot, n);
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
        debug!("get_slot_checkpoints_by_slot(): {:?}", slots);
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
}
