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

use tinyjson::JsonValue;

use darkfi::{Error, Result};
use darkfi_sdk::blockchain::block_epoch;

use crate::ExplorerDb;

#[derive(Debug, Clone)]
/// Structure representing basic statistic extracted from the database.
pub struct BaseStatistics {
    /// Current blockchain height
    pub height: u32,
    /// Current blockchain epoch (based on current height)
    pub epoch: u8,
    /// Blockchains' last block hash
    pub last_block: String,
    /// Blockchain total blocks
    pub total_blocks: usize,
    /// Blockchain total transactions
    pub total_txs: usize,
}

impl BaseStatistics {
    /// Auxiliary function to convert `BaseStatistics` into a `JsonValue` array.
    pub fn to_json_array(&self) -> JsonValue {
        JsonValue::Array(vec![
            JsonValue::Number(self.height as f64),
            JsonValue::Number(self.epoch as f64),
            JsonValue::String(self.last_block.clone()),
            JsonValue::Number(self.total_blocks as f64),
            JsonValue::Number(self.total_txs as f64),
        ])
    }
}

impl ExplorerDb {
    /// Fetch current database basic statistics.
    pub fn get_base_statistics(&self) -> Result<Option<BaseStatistics>> {
        let last_block = self.last_block();
        Ok(last_block
            // Throw database error if last_block retrievals fails
            .map_err(|e| {
                Error::DatabaseError(format!(
                    "[get_base_statistics] Retrieving last block failed: {:?}",
                    e
                ))
            })?
            // Calculate base statistics and return result
            .map(|(height, header_hash)| {
                let epoch = block_epoch(height);
                let total_blocks = self.get_block_count();
                let total_txs = self.get_transaction_count();
                BaseStatistics { height, epoch, last_block: header_hash, total_blocks, total_txs }
            }))
    }
}
