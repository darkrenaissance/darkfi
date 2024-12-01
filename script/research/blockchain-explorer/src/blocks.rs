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

use log::{debug, info};
use tinyjson::JsonValue;

use darkfi::{
    blockchain::{
        BlockInfo, BlockchainOverlay, HeaderHash, SLED_BLOCK_DIFFICULTY_TREE,
        SLED_BLOCK_ORDER_TREE, SLED_BLOCK_TREE,
    },
    util::time::Timestamp,
    Error, Result,
};
use darkfi_sdk::crypto::schnorr::Signature;

use crate::ExplorerDb;

#[derive(Debug, Clone)]
/// Structure representing a block record.
pub struct BlockRecord {
    /// Header hash identifier of the block
    pub header_hash: String,
    /// Block version
    pub version: u8,
    /// Previous block hash
    pub previous: String,
    /// Block height
    pub height: u32,
    /// Block creation timestamp
    pub timestamp: Timestamp,
    /// The block's nonce. This value changes arbitrarily with mining.
    pub nonce: u64,
    /// Merkle tree root of the transactions hashes contained in this block
    pub root: String,
    /// Block producer signature
    pub signature: Signature,
}

impl BlockRecord {
    /// Auxiliary function to convert a `BlockRecord` into a `JsonValue` array.
    pub fn to_json_array(&self) -> JsonValue {
        JsonValue::Array(vec![
            JsonValue::String(self.header_hash.clone()),
            JsonValue::Number(self.version as f64),
            JsonValue::String(self.previous.clone()),
            JsonValue::Number(self.height as f64),
            JsonValue::String(self.timestamp.to_string()),
            JsonValue::Number(self.nonce as f64),
            JsonValue::String(self.root.clone()),
            JsonValue::String(format!("{:?}", self.signature)),
        ])
    }
}

impl From<&BlockInfo> for BlockRecord {
    fn from(block: &BlockInfo) -> Self {
        Self {
            header_hash: block.hash().to_string(),
            version: block.header.version,
            previous: block.header.previous.to_string(),
            height: block.header.height,
            timestamp: block.header.timestamp,
            nonce: block.header.nonce,
            root: block.header.root.to_string(),
            signature: block.signature,
        }
    }
}

impl ExplorerDb {
    /// Resets blocks in the database by clearing all block related trees, returning an Ok result on success.
    pub fn reset_blocks(&self) -> Result<()> {
        let db = &self.blockchain.sled_db;
        // Initialize block related trees to reset
        let trees_to_reset = [SLED_BLOCK_TREE, SLED_BLOCK_ORDER_TREE, SLED_BLOCK_DIFFICULTY_TREE];

        // Iterate over each tree and remove its entries
        for tree_name in &trees_to_reset {
            let tree = db.open_tree(tree_name)?;
            tree.clear()?;
            let tree_name_str = std::str::from_utf8(tree_name)?;
            info!(target: "blockchain-explorer::blocks", "Successfully reset block tree: {tree_name_str}");
        }

        Ok(())
    }

    /// Adds the provided [`BlockInfo`] to the block explorer database.
    ///
    /// This function processes each transaction in the block, calculating and updating the
    /// latest [`GasMetrics`] for non-genesis blocks and for transactions that are not
    /// PoW rewards. After processing all transactions, the block is permanently persisted to
    /// the explorer database.
    pub async fn put_block(&self, block: &BlockInfo) -> Result<()> {
        let blockchain_overlay = BlockchainOverlay::new(&self.blockchain)?;

        // Initialize collections to hold gas data and transactions that have gas data
        let mut tx_gas_data = Vec::with_capacity(block.txs.len());
        let mut txs_hashes_with_gas_data = Vec::with_capacity(block.txs.len());

        // Calculate gas data for non-PoW reward transactions and non-genesis blocks
        for (i, tx) in block.txs.iter().enumerate() {
            if !tx.is_pow_reward() && block.header.height != 0 {
                tx_gas_data.insert(i, self.calculate_tx_gas_data(tx, false).await?);
                txs_hashes_with_gas_data.insert(i, tx.hash());
            }
        }

        // If the block contains transaction gas data, insert the gas metrics into the metrics store
        if !tx_gas_data.is_empty() {
            self.metrics_store.insert_gas_metrics(
                block.header.height,
                &block.header.timestamp,
                &txs_hashes_with_gas_data,
                &tx_gas_data,
            )?;
        }

        // Add the block and commit the changes to persist it
        let _ = blockchain_overlay.lock().unwrap().add_block(block)?;
        blockchain_overlay.lock().unwrap().overlay.lock().unwrap().apply()?;
        debug!(target: "blockchain_explorer::blocks::put_block", "Added block {:?}", block);

        Ok(())
    }

    /// Provides the total block count.
    pub fn get_block_count(&self) -> usize {
        self.blockchain.len()
    }

    /// Fetch all known blocks from the database.
    pub fn get_blocks(&self) -> Result<Vec<BlockRecord>> {
        // Fetch blocks and handle any errors encountered
        let blocks = &self.blockchain.get_all().map_err(|e| {
            Error::DatabaseError(format!("[get_blocks] Block retrieval failed: {e:?}"))
        })?;

        // Transform the found blocks into a vector of block records
        let block_records: Vec<BlockRecord> = blocks.iter().map(BlockRecord::from).collect();

        Ok(block_records)
    }

    /// Fetch a block given its header hash from the database.
    pub fn get_block_by_hash(&self, header_hash: &str) -> Result<Option<BlockRecord>> {
        // Parse header hash, returning an error if parsing fails
        let header_hash = header_hash
            .parse::<HeaderHash>()
            .map_err(|_| Error::ParseFailed("[get_block_by_hash] Invalid header hash"))?;

        // Fetch block by hash and handle encountered errors
        match self.blockchain.get_blocks_by_hash(&[header_hash]) {
            Ok(blocks) => Ok(Some(BlockRecord::from(&blocks[0]))),
            Err(Error::BlockNotFound(_)) => Ok(None),
            Err(e) => Err(Error::DatabaseError(format!(
                "[get_block_by_hash] Block retrieval failed: {e:?}"
            ))),
        }
    }

    /// Fetch the last block from the database.
    pub fn last_block(&self) -> Result<Option<(u32, String)>> {
        let block_store = &self.blockchain.blocks;

        // Return None result when no blocks exist
        if block_store.is_empty() {
            return Ok(None);
        }

        // Blocks exist, retrieve last block
        let (height, header_hash) = block_store.get_last().map_err(|e| {
            Error::DatabaseError(format!("[last_block] Block retrieval failed: {e:?}"))
        })?;

        // Convert header hash to a string and return result
        Ok(Some((height, header_hash.to_string())))
    }

    /// Fetch the last N blocks from the database.
    pub fn get_last_n(&self, n: usize) -> Result<Vec<BlockRecord>> {
        // Fetch the last n blocks and handle any errors encountered
        let blocks_result = &self.blockchain.get_last_n(n).map_err(|e| {
            Error::DatabaseError(format!("[get_last_n] Block retrieval failed: {e:?}"))
        })?;

        // Transform the found blocks into a vector of block records
        let block_records: Vec<BlockRecord> = blocks_result.iter().map(BlockRecord::from).collect();

        Ok(block_records)
    }

    /// Fetch blocks within a specified range from the database.
    pub fn get_by_range(&self, start: u32, end: u32) -> Result<Vec<BlockRecord>> {
        // Fetch blocks in the specified range and handle any errors encountered
        let blocks_result = &self.blockchain.get_by_range(start, end).map_err(|e| {
            Error::DatabaseError(format!("[get_by_range]: Block retrieval failed: {e:?}"))
        })?;

        // Transform the found blocks into a vector of block records
        let block_records: Vec<BlockRecord> = blocks_result.iter().map(BlockRecord::from).collect();

        Ok(block_records)
    }
}
