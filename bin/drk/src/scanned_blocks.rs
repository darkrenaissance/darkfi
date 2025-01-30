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

use rusqlite::types::Value;

use crate::{
    convert_named_params,
    error::{WalletDbError, WalletDbResult},
    Drk,
};

// Wallet SQL table constant names. These have to represent the `wallet.sql`
// SQL schema.
const WALLET_SCANNED_BLOCKS_TABLE: &str = "scanned_blocks";
const WALLET_SCANNED_BLOCKS_COL_HEIGH: &str = "height";
const WALLET_SCANNED_BLOCKS_COL_HASH: &str = "hash";
const WALLET_SCANNED_BLOCKS_COL_ROLLBACK_QUERY: &str = "rollback_query";

impl Drk {
    /// Insert a scanned block information record into the wallet.
    pub fn put_scanned_block_record(
        &self,
        height: u32,
        hash: &str,
        rollback_query: &str,
    ) -> WalletDbResult<()> {
        let query = format!(
            "INSERT INTO {} ({}, {}, {}) VALUES (?1, ?2, ?3);",
            WALLET_SCANNED_BLOCKS_TABLE,
            WALLET_SCANNED_BLOCKS_COL_HEIGH,
            WALLET_SCANNED_BLOCKS_COL_HASH,
            WALLET_SCANNED_BLOCKS_COL_ROLLBACK_QUERY,
        );
        self.wallet.exec_sql(&query, rusqlite::params![height, hash, rollback_query])
    }

    /// Auxiliary function to parse a `WALLET_SCANNED_BLOCKS_TABLE` records.
    fn parse_scanned_block_record(&self, row: &[Value]) -> WalletDbResult<(u32, String, String)> {
        let Value::Integer(height) = row[0] else {
            return Err(WalletDbError::ParseColumnValueError);
        };
        let Ok(height) = u32::try_from(height) else {
            return Err(WalletDbError::ParseColumnValueError);
        };

        let Value::Text(ref hash) = row[1] else {
            return Err(WalletDbError::ParseColumnValueError);
        };

        let Value::Text(ref rollback_query) = row[2] else {
            return Err(WalletDbError::ParseColumnValueError);
        };

        Ok((height, hash.clone(), rollback_query.clone()))
    }

    /// Get a scanned block information record.
    pub fn get_scanned_block_record(&self, height: u32) -> WalletDbResult<(u32, String, String)> {
        let row = self.wallet.query_single(
            WALLET_SCANNED_BLOCKS_TABLE,
            &[],
            convert_named_params! {(WALLET_SCANNED_BLOCKS_COL_HEIGH, height)},
        )?;

        self.parse_scanned_block_record(&row)
    }

    /// Fetch all scanned block information record.
    pub fn get_scanned_block_records(&self) -> WalletDbResult<Vec<(u32, String, String)>> {
        let rows = self.wallet.query_multiple(WALLET_SCANNED_BLOCKS_TABLE, &[], &[])?;

        let mut ret = Vec::with_capacity(rows.len());
        for row in rows {
            ret.push(self.parse_scanned_block_record(&row)?);
        }

        Ok(ret)
    }

    /// Get the last scanned block height and hash from the wallet.
    /// If database is empty default (0, '-') is returned.
    pub fn get_last_scanned_block(&self) -> WalletDbResult<(u32, String)> {
        let query = format!(
            "SELECT * FROM {} ORDER BY {} DESC LIMIT 1;",
            WALLET_SCANNED_BLOCKS_TABLE, WALLET_SCANNED_BLOCKS_COL_HEIGH,
        );
        let ret = self.wallet.query_custom(&query, &[])?;

        if ret.is_empty() {
            return Ok((0, String::from("-")))
        }

        let (height, hash, _) = self.parse_scanned_block_record(&ret[0])?;

        Ok((height, hash))
    }

    /// Reset the scanned blocks information records in the wallet.
    pub fn reset_scanned_blocks(&self) -> WalletDbResult<()> {
        println!("Resetting scanned blocks");
        let query = format!("DELETE FROM {};", WALLET_SCANNED_BLOCKS_TABLE);
        self.wallet.exec_sql(&query, &[])?;
        println!("Successfully reset scanned blocks");

        Ok(())
    }

    /// Reset state to provided block height.
    /// If genesis block height(0) was provided, perform a full reset.
    pub async fn reset_to_height(&self, height: u32) -> WalletDbResult<()> {
        println!("Resetting wallet state to block: {height}");

        // If genesis block height(0) was provided,
        // perform a full reset.
        if height == 0 {
            return self.reset().await
        }

        // Grab last scanned block height
        let (last, _) = self.get_last_scanned_block()?;

        // Check if requested height is after it
        if last <= height {
            println!("Requested block height is greater or equal to last scanned block");
            return Ok(())
        }

        // Iterate the range (height, last] in reverse to grab the corresponding blocks
        for height in (height + 1..=last).rev() {
            let (height, hash, query) = self.get_scanned_block_record(height)?;
            println!("Reverting block: {height} - {hash}");
            self.wallet.exec_batch_sql(&query)?;
            let query = format!(
                "DELETE FROM {} WHERE {} = {};",
                WALLET_SCANNED_BLOCKS_TABLE, WALLET_SCANNED_BLOCKS_COL_HEIGH, height
            );
            self.wallet.exec_batch_sql(&query)?;
        }

        println!("Successfully reset wallet state");
        Ok(())
    }
}
