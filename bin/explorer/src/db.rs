/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::io;

use bytemuck::{Pod, Zeroable};
use darkfi::{
    blockchain::{BlockInfo, Header},
    tx::Transaction,
};
use darkfi_sdk::crypto::schnorr::Signature;
use darkfi_serial::{deserialize_async, serialize_async};
use sled::{transaction::TransactionError, Transactional};
use tapes::{BlobTape, FixedSizedTape, Persistence, TapeOpenOptions, Tapes};
use tracing::info;

use super::Explorer;

/// Index entry for a block pointing to block data in the blob tape
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
#[repr(C)]
pub struct BlockIndex {
    pub offset: u64,
    pub length: u64,
    pub tx_count: u64,
    pub tx_start_idx: u64,
}

/// Index entry for a transaction pointing to tx data in the blob tape
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
#[repr(C)]
pub struct TxIndex {
    pub offset: u64,
    pub length: u64,
    pub block_height: u64,
}

/// Difficulty data for a block
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
#[repr(C)]
pub struct DifficultyIndex {
    pub difficulty: u64,
    pub cumulative: u64,
}

/// Structure holding all tapes in the database.
pub struct TapesDatabase {
    pub block_index: FixedSizedTape<BlockIndex>,
    pub tx_index: FixedSizedTape<TxIndex>,
    pub difficulty_index: FixedSizedTape<DifficultyIndex>,
    pub blocks: BlobTape,
    pub transactions: BlobTape,
}

impl Explorer {
    pub fn open_tapes(db: &Tapes, options: &TapeOpenOptions) -> io::Result<TapesDatabase> {
        let mut tx = db.append();
        let block_index = tx.open_fixed_sized_tape("block_index", options)?;
        let tx_index = tx.open_fixed_sized_tape("tx_index", options)?;
        let difficulty_index = tx.open_fixed_sized_tape("diff_index", options)?;
        let blocks = tx.open_blob_tape("blocks", options)?;
        let transactions = tx.open_blob_tape("transactions", options)?;
        tx.commit(Persistence::Buffer)?;
        Ok(TapesDatabase { block_index, tx_index, difficulty_index, blocks, transactions })
    }

    /// Append a new block
    pub async fn append_block(&self, block: &BlockInfo, diff: &DifficultyIndex) -> io::Result<()> {
        let mut tx = self.tapes_db.append();

        let block_offset = tx.blob_tape_len(&self.database.blocks).unwrap_or(0);
        let tx_blob_offset = tx.blob_tape_len(&self.database.transactions).unwrap_or(0);
        let tx_start_idx = tx.fixed_sized_tape_len(&self.database.tx_index).unwrap_or(0);

        // Append block header
        let header_data = serialize_async(&block.header).await;
        tx.append_bytes(&self.database.blocks, &header_data)?;

        // Append all block transactions
        let mut current_tx_offset = tx_blob_offset;
        for transaction in &block.txs {
            let tx_data = serialize_async(transaction).await;
            tx.append_bytes(&self.database.transactions, &tx_data)?;

            let tx_idx = TxIndex {
                offset: current_tx_offset,
                length: tx_data.len() as u64,
                block_height: block.header.height as u64,
            };
            tx.append_entries(&self.database.tx_index, std::slice::from_ref(&tx_idx))?;

            current_tx_offset += tx_data.len() as u64;
        }

        // Append block index
        let block_idx = BlockIndex {
            offset: block_offset,
            length: header_data.len() as u64,
            tx_count: block.txs.len() as u64,
            tx_start_idx,
        };
        tx.append_entries(&self.database.block_index, std::slice::from_ref(&block_idx))?;

        // Append difficulty
        tx.append_entries(&self.database.difficulty_index, std::slice::from_ref(diff))?;

        // Commit Tapes first
        tx.commit(Persistence::SyncData)?;

        // Prepare data for atomic sled transaction
        let header_hash = serialize_async(&block.header.hash()).await;
        // Store height as u64 (8 bytes) to match lookup format
        let height_bytes = (block.header.height as u64).to_le_bytes();

        // Collect tx hashes and their indices
        let mut tx_entries: Vec<([u8; 32], [u8; 8])> = Vec::with_capacity(block.txs.len());
        for (i, transaction) in block.txs.iter().enumerate() {
            let tx_hash = *transaction.hash().inner();
            let tx_idx_pos = tx_start_idx + i as u64;
            tx_entries.push((tx_hash, tx_idx_pos.to_le_bytes()));
        }

        // Atomic sled transaction for both tx_indices and header_indices
        (&self.tx_indices, &self.header_indices)
            .transaction(|(tx_tree, header_tree)| {
                // Insert all transaction indices
                for (hash, idx) in &tx_entries {
                    tx_tree.insert(hash.as_slice(), idx.as_slice())?;
                }
                // Insert header hash -> height mapping
                header_tree.insert(header_hash.as_slice(), height_bytes.as_slice())?;
                Ok(())
            })
            .map_err(|e: TransactionError<sled::Error>| {
                io::Error::other(format!("sled transaction error: {e}"))
            })?;

        info!(
            "Appended block {} ({} bytes header, {} txs)",
            block.header.height,
            header_data.len(),
            block.txs.len(),
        );

        Ok(())
    }

    /// Revert n blocks from the database
    pub async fn revert_blocks(&self, count: u64) -> io::Result<()> {
        if count == 0 {
            return Ok(())
        }

        let reader = self.tapes_db.reader();
        let current_len = reader.fixed_sized_tape_len(&self.database.block_index).unwrap_or(0);

        if count > current_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Cannot revert more blocks than exist",
            ))
        }

        let new_block_count = current_len - count;
        let current_tx_idx_len = reader.fixed_sized_tape_len(&self.database.tx_index).unwrap_or(0);

        // Collect data to remove from sled
        let mut header_hashes_to_remove: Vec<Vec<u8>> = Vec::new();
        let mut tx_hashes_to_remove: Vec<Vec<u8>> = Vec::new();

        for height in new_block_count..current_len {
            // Get the block index to find transactions
            let block_idx = reader
                .read_entry(&self.database.block_index, height)?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "block index not found"))?;

            // Read header to get its hash
            let mut header_data = vec![0u8; block_idx.length as usize];
            reader.read_bytes(&self.database.blocks, block_idx.offset, &mut header_data)?;
            let header: Header = deserialize_async(&header_data).await?;
            header_hashes_to_remove.push(serialize_async(&header.hash()).await);

            // Read each tx to get its hash
            for i in 0..block_idx.tx_count {
                let tx_idx = reader
                    .read_entry(&self.database.tx_index, block_idx.tx_start_idx + i)?
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "tx index not found"))?;

                let mut tx_data = vec![0u8; tx_idx.length as usize];
                reader.read_bytes(&self.database.transactions, tx_idx.offset, &mut tx_data)?;
                let transaction: Transaction = deserialize_async(&tx_data).await?;
                tx_hashes_to_remove.push(transaction.hash().0.to_vec());
            }
        }

        let (new_block_blob_len, new_tx_idx_len, new_tx_blob_len) = if new_block_count == 0 {
            (0, 0, 0)
        } else {
            let last_block_idx = reader
                .read_entry(&self.database.block_index, new_block_count - 1)?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "block index not found"))?;

            let new_block_blob_len = last_block_idx.offset + last_block_idx.length;
            let new_tx_idx_len = last_block_idx.tx_start_idx + last_block_idx.tx_count;

            let new_tx_blob_len = if new_tx_idx_len == 0 {
                0
            } else {
                let last_tx_idx =
                    reader.read_entry(&self.database.tx_index, new_tx_idx_len - 1)?.ok_or_else(
                        || io::Error::new(io::ErrorKind::NotFound, "tx index not found"),
                    )?;
                last_tx_idx.offset + last_tx_idx.length
            };

            (new_block_blob_len, new_tx_idx_len, new_tx_blob_len)
        };

        // Drop the reader before truncating
        drop(reader);

        let mut truncate_tx = self.tapes_db.truncate();
        truncate_tx.drop_from_fixed_sized_tape(&self.database.block_index, count);
        truncate_tx.drop_from_fixed_sized_tape(&self.database.difficulty_index, count);

        let tx_idx_to_remove = current_tx_idx_len - new_tx_idx_len;
        truncate_tx.drop_from_fixed_sized_tape(&self.database.tx_index, tx_idx_to_remove);

        truncate_tx.set_blob_tape_len(&self.database.blocks, new_block_blob_len);
        truncate_tx.set_blob_tape_len(&self.database.transactions, new_tx_blob_len);

        truncate_tx.commit(Persistence::SyncData)?;

        // Atomic sled transaction for removing both tx and header indices
        (&self.tx_indices, &self.header_indices)
            .transaction(|(tx_tree, header_tree)| {
                for tx_hash in &tx_hashes_to_remove {
                    tx_tree.remove(tx_hash.as_slice())?;
                }
                for header_hash in &header_hashes_to_remove {
                    header_tree.remove(header_hash.as_slice())?;
                }
                Ok(())
            })
            .map_err(|e: TransactionError<sled::Error>| {
                io::Error::other(format!("sled transaction error: {e}"))
            })?;

        info!(
            "Reverted {} blocks (new height: {})",
            count,
            if new_block_count == 0 { 0 } else { new_block_count - 1 }
        );

        Ok(())
    }

    /// Revert to a specific height (keep blocks 0..=target_height)
    pub async fn revert_to_height(&self, target_height: u64) -> io::Result<()> {
        let current_height = self.get_height()?.unwrap_or(0);
        if target_height >= current_height {
            return Ok(())
        }
        self.revert_blocks(current_height - target_height).await
    }

    /// Get the current known blockchain height
    pub fn get_height(&self) -> io::Result<Option<u64>> {
        let reader = self.tapes_db.reader();
        let len = reader.fixed_sized_tape_len(&self.database.block_index);
        Ok(len.filter(|&l| l > 0).map(|l| l - 1))
    }

    /// Get the difficulty and cumulative difficulty for a block height
    pub fn get_difficulty(&self, height: u64) -> io::Result<Option<DifficultyIndex>> {
        let reader = self.tapes_db.reader();
        reader.read_entry(&self.database.difficulty_index, height)
    }

    /// Get the block header for a height
    pub async fn get_header(&self, height: u64) -> io::Result<Option<Header>> {
        let reader = self.tapes_db.reader();

        let idx = match reader.read_entry(&self.database.block_index, height)? {
            Some(idx) => idx,
            None => return Ok(None),
        };

        let mut data = vec![0u8; idx.length as usize];
        reader.read_bytes(&self.database.blocks, idx.offset, &mut data)?;

        Ok(Some(deserialize_async(&data).await?))
    }

    /// Get all the transactions in a given block height
    pub async fn get_block_txs(&self, height: u64) -> io::Result<Option<Vec<Transaction>>> {
        let reader = self.tapes_db.reader();

        let block_idx = match reader.read_entry(&self.database.block_index, height)? {
            Some(idx) => idx,
            None => return Ok(None),
        };

        if block_idx.tx_count == 0 {
            return Ok(Some(vec![]))
        }

        // Read all TxIndex entries for this block
        let mut tx_indices = Vec::with_capacity(block_idx.tx_count as usize);
        for i in 0..block_idx.tx_count {
            let tx_idx = reader
                .read_entry(&self.database.tx_index, block_idx.tx_start_idx + i)?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "tx index not found"))?;
            tx_indices.push(tx_idx);
        }

        if tx_indices.is_empty() {
            return Ok(Some(vec![]))
        }

        // Since transactions are stored contiguously, read all data at once
        let first_tx = &tx_indices[0];
        let last_tx = &tx_indices[tx_indices.len() - 1];
        let total_len = (last_tx.offset + last_tx.length - first_tx.offset) as usize;

        // Read all transaction data in one operation
        let mut all_tx_data = vec![0u8; total_len];
        reader.read_bytes(&self.database.transactions, first_tx.offset, &mut all_tx_data)?;

        // Deserialize each transaction from the combined buffer
        let mut txs = Vec::with_capacity(tx_indices.len());
        for tx_idx in &tx_indices {
            let start = (tx_idx.offset - first_tx.offset) as usize;
            let end = start + tx_idx.length as usize;
            txs.push(deserialize_async(&all_tx_data[start..end]).await?);
        }

        Ok(Some(txs))
    }

    /// Get and construct the entire block for a given height.
    pub async fn get_block(&self, height: u64) -> io::Result<Option<BlockInfo>> {
        let header = match self.get_header(height).await? {
            Some(h) => h,
            None => return Ok(None),
        };

        let txs = self.get_block_txs(height).await?.unwrap_or_default();

        // We don't care about displaying the block signature.
        Ok(Some(BlockInfo { header, txs, signature: Signature::dummy() }))
    }

    /// Get basic block info without loading all transactions.
    /// Returns (header, tx_count, total_size) for efficient latest_blocks display.
    pub async fn get_block_summary(&self, height: u64) -> io::Result<Option<(Header, u64, u64)>> {
        let reader = self.tapes_db.reader();

        let block_idx = match reader.read_entry(&self.database.block_index, height)? {
            Some(idx) => idx,
            None => return Ok(None),
        };

        let mut header_data = vec![0u8; block_idx.length as usize];
        reader.read_bytes(&self.database.blocks, block_idx.offset, &mut header_data)?;
        let header: Header = deserialize_async(&header_data).await?;

        // Calculate total size: header + all transactions
        let total_tx_size = if block_idx.tx_count == 0 {
            0
        } else {
            let first_tx_idx = reader
                .read_entry(&self.database.tx_index, block_idx.tx_start_idx)?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "tx index not found"))?;
            let last_tx_idx = reader
                .read_entry(
                    &self.database.tx_index,
                    block_idx.tx_start_idx + block_idx.tx_count - 1,
                )?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "tx index not found"))?;
            last_tx_idx.offset + last_tx_idx.length - first_tx_idx.offset
        };

        let total_size = block_idx.length + total_tx_size;
        Ok(Some((header, block_idx.tx_count, total_size)))
    }

    /// Get a transaction by its hash.
    /// Returns the transaction and the block height it belongs to.
    pub async fn get_tx_by_hash(
        &self,
        tx_hash: &[u8; 32],
    ) -> io::Result<Option<(Transaction, u64)>> {
        // Look up the tx_index position from sled
        let tx_idx_pos = match self.tx_indices.get(tx_hash)? {
            Some(pos_bytes) => {
                let bytes: [u8; 8] = pos_bytes.as_ref().try_into().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid tx index position")
                })?;
                u64::from_le_bytes(bytes)
            }
            None => return Ok(None),
        };

        // Read the TxIndex from tapes
        let reader = self.tapes_db.reader();
        let tx_idx = match reader.read_entry(&self.database.tx_index, tx_idx_pos)? {
            Some(idx) => idx,
            None => return Ok(None),
        };

        // Read the transaction data from the blob tape
        let mut data = vec![0u8; tx_idx.length as usize];
        reader.read_bytes(&self.database.transactions, tx_idx.offset, &mut data)?;

        let transaction: Transaction = deserialize_async(&data).await?;
        Ok(Some((transaction, tx_idx.block_height)))
    }

    /// Get a transaction by its hash string (hex encoded).
    pub async fn get_tx_by_hash_str(
        &self,
        tx_hash_str: &str,
    ) -> io::Result<Option<(Transaction, u64)>> {
        let hash_bytes = hex::decode(tx_hash_str)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid hex string"))?;

        if hash_bytes.len() != 32 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "hash must be 32 bytes"));
        }

        let mut tx_hash = [0u8; 32];
        tx_hash.copy_from_slice(&hash_bytes);

        self.get_tx_by_hash(&tx_hash).await
    }
}
