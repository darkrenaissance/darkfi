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

use std::{io, str::FromStr};

use bytemuck::{Pod, Zeroable};
use darkfi::{
    blockchain::{BlockInfo, Header},
    tx::Transaction,
};
use darkfi_deployooor_contract::{model::LockParamsV1, DeployFunction};
use darkfi_sdk::{
    crypto::{schnorr::Signature, ContractId, DEPLOYOOOR_CONTRACT_ID},
    deploy::DeployParamsV1,
};
use darkfi_serial::{
    async_trait, deserialize, deserialize_async, serialize, serialize_async, SerialDecodable,
    SerialEncodable,
};
use sled::{transaction::TransactionError, Transactional};
use tapes::{BlobTape, FixedSizedTape, Persistence, TapeOpenOptions, Tapes};
use tracing::info;

use super::Explorer;

/// Contract information stored in sled
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ContractData {
    pub contract_id: ContractId,
    pub locked: bool,
    pub wasm_size: u64,
    pub deploy_block: u64,
    pub deploy_tx_hash: [u8; 32],
}

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

        // Scan for contract deployments and locks
        let (new_contracts, locked_contracts) =
            self.scan_contract_calls(block, block.header.height as u64).await;

        // Atomic sled transaction for tx_indices, header_indices, and contracts
        (&self.tx_indices, &self.header_indices, &self.contracts)
            .transaction(|(tx_tree, header_tree, contracts_tree)| {
                // Insert all transaction indices
                for (hash, idx) in &tx_entries {
                    tx_tree.insert(hash.as_slice(), idx.as_slice())?;
                }
                // Insert header hash -> height mapping
                header_tree.insert(header_hash.as_slice(), height_bytes.as_slice())?;

                // Insert new contracts
                for contract in &new_contracts {
                    contracts_tree
                        .insert(serialize(&contract.contract_id.inner()), serialize(contract))?;
                }

                // Update locked contracts
                for contract_id in &locked_contracts {
                    let data = contracts_tree.get(serialize(&contract_id.inner()))?.unwrap();
                    let mut contract: ContractData = deserialize(&data).unwrap();
                    contract.locked = true;
                    contracts_tree.insert(serialize(&contract_id.inner()), serialize(&contract))?;
                }

                Ok(())
            })
            .map_err(|e: TransactionError<sled::Error>| {
                io::Error::other(format!("sled transaction error: {e}"))
            })?;

        info!(
            target: "explorer::append_block",
            "Appended block {} ({} bytes header, {} txs)",
            block.header.height,
            header_data.len(),
            block.txs.len(),
        );

        // Update stats
        let block_size = header_data.len() as u64 + (current_tx_offset - tx_blob_offset);
        self.update_stats_for_block(
            block.header.timestamp.inner(),
            block.txs.len() as u64,
            block_size,
        )
        .await?;

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
        let mut contracts_to_remove: Vec<ContractId> = Vec::new();

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

            // Read each tx to get its hash and check for contract deployments
            for i in 0..block_idx.tx_count {
                let tx_idx = reader
                    .read_entry(&self.database.tx_index, block_idx.tx_start_idx + i)?
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "tx index not found"))?;

                let mut tx_data = vec![0u8; tx_idx.length as usize];
                reader.read_bytes(&self.database.transactions, tx_idx.offset, &mut tx_data)?;
                let transaction: Transaction = deserialize_async(&tx_data).await?;
                tx_hashes_to_remove.push(transaction.hash().0.to_vec());

                // Check for contract deployments to remove
                for call in &transaction.calls {
                    if call.data.contract_id == *DEPLOYOOOR_CONTRACT_ID &&
                        call.data.data[0] == DeployFunction::DeployV1 as u8
                    {
                        let params: DeployParamsV1 =
                            deserialize_async(&call.data.data[1..]).await?;
                        contracts_to_remove.push(ContractId::derive_public(params.public_key));
                    }
                }
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

        // Atomic sled transaction for removing tx, header indices, and contracts
        (&self.tx_indices, &self.header_indices, &self.contracts)
            .transaction(|(tx_tree, header_tree, contracts_tree)| {
                for tx_hash in &tx_hashes_to_remove {
                    tx_tree.remove(tx_hash.as_slice())?;
                }
                for header_hash in &header_hashes_to_remove {
                    header_tree.remove(header_hash.as_slice())?;
                }
                for contract_id in &contracts_to_remove {
                    contracts_tree.remove(serialize(&contract_id.inner()))?;
                }
                Ok(())
            })
            .map_err(|e: TransactionError<sled::Error>| {
                io::Error::other(format!("sled transaction error: {e}"))
            })?;

        info!(
            target: "explorer::revert_blocks",
            "Reverted {} blocks (new height: {})",
            count,
            if new_block_count == 0 { 0 } else { new_block_count - 1 }
        );

        // Rebuild stats from scratch after reorg
        self.rebuild_stats().await?;

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

    /// Scan a block's transactions for contract deployments and locks.
    /// Returns (new_contracts, locked_contract_ids)
    async fn scan_contract_calls(
        &self,
        block: &BlockInfo,
        block_height: u64,
    ) -> (Vec<ContractData>, Vec<ContractId>) {
        let mut new_contracts = Vec::new();
        let mut locked_contracts = Vec::new();

        for transaction in &block.txs {
            let tx_hash = *transaction.hash().inner();

            for call in &transaction.calls {
                // Check if this is a call to Deployoor
                if call.data.contract_id != *DEPLOYOOOR_CONTRACT_ID {
                    continue;
                }

                let func = call.data.data[0];
                if func == DeployFunction::DeployV1 as u8 {
                    let params: DeployParamsV1 =
                        deserialize_async(&call.data.data[1..]).await.unwrap();
                    let contract_id = ContractId::derive_public(params.public_key);

                    info!(
                        target: "explorer::scan_contract_calls",
                        "Found contract deployment: {} (size: {} bytes)",
                        contract_id, params.wasm_bincode.len(),
                    );

                    new_contracts.push(ContractData {
                        contract_id,
                        locked: false,
                        wasm_size: params.wasm_bincode.len() as u64,
                        deploy_block: block_height,
                        deploy_tx_hash: tx_hash,
                    });
                } else if func == DeployFunction::LockV1 as u8 {
                    let params: LockParamsV1 =
                        deserialize_async(&call.data.data[1..]).await.unwrap();
                    let contract_id = ContractId::derive_public(params.public_key);

                    info!(
                        target: "explorer::scan_contract_calls",
                        "Found contract lock: {}", contract_id,
                    );

                    locked_contracts.push(contract_id);
                }
            }
        }

        (new_contracts, locked_contracts)
    }

    /// Get a contract by its ID string (base58 encoded).
    pub async fn get_contract(&self, contract_id_str: &str) -> io::Result<Option<ContractData>> {
        let Ok(contract_id) = ContractId::from_str(contract_id_str) else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid contract ID"))
        };

        match self.contracts.get(serialize_async(&contract_id.inner()).await)? {
            Some(data) => Ok(Some(deserialize_async(&data).await?)),
            None => Ok(None),
        }
    }

    /// List all contracts, optionally filtered by locked status.
    pub async fn list_contracts(
        &self,
        locked_filter: Option<bool>,
    ) -> io::Result<Vec<ContractData>> {
        let mut contracts = Vec::new();

        for result in self.contracts.iter() {
            let (_, value) = result?;
            let contract: ContractData = deserialize_async(&value).await?;
            if let Some(filter) = locked_filter {
                if contract.locked == filter {
                    contracts.push(contract);
                }
            } else {
                contracts.push(contract);
            }
        }

        Ok(contracts)
    }

    /// Get the total number of contracts.
    pub fn get_contract_count(&self) -> io::Result<u64> {
        Ok(self.contracts.len() as u64)
    }
}

/// Daily statistics aggregate
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DailyStats {
    pub block_count: u64,
    pub user_tx_count: u64, // excluding coinbase
    pub total_size: u64,
}

/// Monthly statistics aggregate
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MonthlyStats {
    pub block_count: u64,
    pub total_size: u64,
}

impl Explorer {
    /// Update stats for a single block
    pub async fn update_stats_for_block(
        &self,
        timestamp: u64,
        tx_count: u64,
        block_size: u64,
    ) -> io::Result<()> {
        let day = Self::day_from_timestamp(timestamp);
        let (year, month) = Self::year_month_from_timestamp(timestamp);
        let user_tx = tx_count.saturating_sub(1); // exclude coinbase

        // Update daily stats
        let daily_key = format!("daily:{}", day);
        let mut daily = self.get_daily_stats(day).await?.unwrap_or(DailyStats {
            block_count: 0,
            user_tx_count: 0,
            total_size: 0,
        });
        daily.block_count += 1;
        daily.user_tx_count += user_tx;
        daily.total_size += block_size;
        self.stats.insert(daily_key.as_bytes(), serialize_async(&daily).await)?;

        // Update monthly stats
        let monthly_key = format!("monthly:{}:{:02}", year, month);
        let mut monthly = self
            .get_monthly_stats(year, month)
            .await?
            .unwrap_or(MonthlyStats { block_count: 0, total_size: 0 });
        monthly.block_count += 1;
        monthly.total_size += block_size;
        self.stats.insert(monthly_key.as_bytes(), serialize_async(&monthly).await)?;

        Ok(())
    }

    /// Get daily stats for a specific day
    pub async fn get_daily_stats(&self, day: u64) -> io::Result<Option<DailyStats>> {
        let key = format!("daily:{}", day);
        match self.stats.get(key.as_bytes())? {
            Some(data) => Ok(Some(deserialize_async(&data).await?)),
            None => Ok(None),
        }
    }

    /// Get monthly stats for a specific year/month
    pub async fn get_monthly_stats(
        &self,
        year: u32,
        month: u32,
    ) -> io::Result<Option<MonthlyStats>> {
        let key = format!("monthly:{}:{:02}", year, month);
        match self.stats.get(key.as_bytes())? {
            Some(data) => Ok(Some(deserialize_async(&data).await?)),
            None => Ok(None),
        }
    }

    /// Get all daily stats (for graph generation)
    pub async fn get_all_daily_stats(&self) -> io::Result<Vec<(u64, DailyStats)>> {
        let mut result = Vec::new();
        let prefix = b"daily:";

        for item in self.stats.scan_prefix(prefix) {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if let Some(day_str) = key_str.strip_prefix("daily:") {
                if let Ok(day) = day_str.parse::<u64>() {
                    let stats = deserialize_async(&value).await?;
                    result.push((day, stats));
                }
            }
        }

        result.sort_by_key(|(day, _)| *day);
        Ok(result)
    }

    /// Get all monthly stats (for table generation)
    pub async fn get_all_monthly_stats(&self) -> io::Result<Vec<(u32, u32, MonthlyStats)>> {
        let mut result = Vec::new();
        let prefix = b"monthly:";

        for item in self.stats.scan_prefix(prefix) {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if let Some(ym_str) = key_str.strip_prefix("monthly:") {
                let parts: Vec<&str> = ym_str.split(':').collect();
                if parts.len() == 2 {
                    if let (Ok(year), Ok(month)) =
                        (parts[0].parse::<u32>(), parts[1].parse::<u32>())
                    {
                        let stats = deserialize_async(&value).await?;
                        result.push((year, month, stats));
                    }
                }
            }
        }

        result.sort_by_key(|(year, month, _)| (*year, *month));
        Ok(result)
    }

    /// Clear all stats (called before rebuilding after reorg)
    pub fn clear_stats(&self) -> io::Result<()> {
        // Clear daily stats
        let daily_keys: Vec<_> =
            self.stats.scan_prefix(b"daily:").filter_map(|r| r.ok().map(|(k, _)| k)).collect();
        for key in daily_keys {
            self.stats.remove(&key)?;
        }

        // Clear monthly stats
        let monthly_keys: Vec<_> =
            self.stats.scan_prefix(b"monthly:").filter_map(|r| r.ok().map(|(k, _)| k)).collect();
        for key in monthly_keys {
            self.stats.remove(&key)?;
        }

        Ok(())
    }

    /// Rebuild all stats from blockchain data
    pub async fn rebuild_stats(&self) -> io::Result<()> {
        info!(target: "explorer::rebuild_stats", "Clearing existing stats...");
        self.clear_stats()?;

        let height = match self.get_height()? {
            Some(h) => h,
            None => return Ok(()), // No blocks yet
        };

        info!(target: "explorer::rebuild_stats", "Rebuilding stats for {} blocks...", height + 1);

        let reader = self.tapes_db.reader();

        for h in 0..=height {
            let block_idx = match reader.read_entry(&self.database.block_index, h)? {
                Some(idx) => idx,
                None => continue,
            };

            // Read header to get timestamp
            let mut header_data = vec![0u8; block_idx.length as usize];
            reader.read_bytes(&self.database.blocks, block_idx.offset, &mut header_data)?;
            let header: Header = deserialize_async(&header_data).await?;

            // Calculate block size
            let tx_size = if block_idx.tx_count == 0 {
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

            let block_size = block_idx.length + tx_size;

            self.update_stats_for_block(header.timestamp.inner(), block_idx.tx_count, block_size)
                .await?;
        }

        info!(target: "explorer::rebuild_stats", "Stats rebuild complete");
        Ok(())
    }

    /// Get day number from unix timestamp (days since epoch)
    fn day_from_timestamp(timestamp: u64) -> u64 {
        timestamp / 86400
    }

    /// Get year and month from unix timestamp
    fn year_month_from_timestamp(timestamp: u64) -> (u32, u32) {
        // Days since epoch
        let days = timestamp / 86400;

        // Approximate year (will be corrected)
        let mut year = 1970u32;
        let mut remaining_days = days as i64;

        loop {
            let days_in_year =
                if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) { 366i64 } else { 365i64 };

            if remaining_days < days_in_year {
                break;
            }
            remaining_days -= days_in_year;
            year += 1;
        }

        // Now find month
        let is_leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let days_in_months: [i64; 12] = if is_leap {
            [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        } else {
            [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        };

        let mut month = 1u32;
        for days_in_month in days_in_months.iter() {
            if remaining_days < *days_in_month {
                break;
            }
            remaining_days -= days_in_month;
            month += 1;
        }

        (year, month)
    }
}
