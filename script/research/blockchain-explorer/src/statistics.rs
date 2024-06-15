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

use rusqlite::types::Value;
use tinyjson::JsonValue;

use darkfi_sdk::blockchain::block_epoch;
use drk::error::{WalletDbError, WalletDbResult};

use crate::{blocks::BLOCKS_TABLE, transactions::TRANSACTIONS_TABLE, BlockchainExplorer};

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
    pub total_blocks: u64,
    /// Blockchain total transactions
    pub total_txs: u64,
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

impl BlockchainExplorer {
    /// Fetch total rows count of given table from the database.
    pub async fn get_table_count(&self, table: &str) -> WalletDbResult<u64> {
        // First we prepare the query
        let query = format!("SELECT COUNT() FROM {};", table);
        let Ok(conn) = self.database.conn.lock() else {
            return Err(WalletDbError::FailedToAquireLock)
        };
        let Ok(mut stmt) = conn.prepare(&query) else {
            return Err(WalletDbError::QueryPreparationFailed)
        };

        // Execute the query using provided params
        let Ok(mut rows) = stmt.query([]) else { return Err(WalletDbError::QueryExecutionFailed) };

        // Check if row exists
        let Ok(next) = rows.next() else { return Err(WalletDbError::QueryExecutionFailed) };
        let row = match next {
            Some(row_result) => row_result,
            None => return Ok(0_u64),
        };

        // Parse returned value
        let Ok(count) = row.get(0) else { return Err(WalletDbError::ParseColumnValueError) };
        let Value::Integer(count) = count else { return Err(WalletDbError::ParseColumnValueError) };
        let Ok(count) = u64::try_from(count) else {
            return Err(WalletDbError::ParseColumnValueError)
        };

        Ok(count)
    }

    /// Fetch current database basic statistic.
    pub async fn get_base_statistics(&self) -> WalletDbResult<BaseStatistics> {
        let (height, last_block) = self.last_block().await?;
        let epoch = block_epoch(height);
        let total_blocks = self.get_table_count(BLOCKS_TABLE).await?;
        let total_txs = self.get_table_count(TRANSACTIONS_TABLE).await?;

        Ok(BaseStatistics { height, epoch, last_block, total_blocks, total_txs })
    }
}
