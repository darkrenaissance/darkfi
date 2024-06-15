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

use log::info;
use rusqlite::types::Value;
use tinyjson::JsonValue;

use darkfi::{blockchain::BlockInfo, Error, Result};
use darkfi_sdk::crypto::schnorr::Signature;
use darkfi_serial::{deserialize, serialize};
use drk::{
    convert_named_params,
    error::{WalletDbError, WalletDbResult},
};

use crate::BlockchainExplorer;

// Dtabase SQL table constant names. These have to represent the `block.sql`
// SQL schema.
pub const BLOCKS_TABLE: &str = "blocks";

// BLOCKS_TABLE
pub const BLOCKS_COL_HEADER_HASH: &str = "header_hash";
pub const BLOCKS_COL_VERSION: &str = "version";
pub const BLOCKS_COL_PREVIOUS: &str = "previous";
pub const BLOCKS_COL_HEIGHT: &str = "height";
pub const BLOCKS_COL_TIMESTAMP: &str = "timestamp";
pub const BLOCKS_COL_NONCE: &str = "nonce";
pub const BLOCKS_COL_ROOT: &str = "root";
pub const BLOCKS_COL_SIGNATURE: &str = "signature";

#[derive(Debug, Clone)]
/// Structure representing a `BLOCKS_TABLE` record.
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
    pub timestamp: u64,
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
        let mut ret = vec![];
        ret.push(JsonValue::String(self.header_hash.clone()));
        ret.push(JsonValue::Number(self.version as f64));
        ret.push(JsonValue::String(self.previous.clone()));
        ret.push(JsonValue::Number(self.height as f64));
        ret.push(JsonValue::Number(self.timestamp as f64));
        ret.push(JsonValue::Number(self.nonce as f64));
        ret.push(JsonValue::String(self.root.clone()));
        ret.push(JsonValue::String(format!("{:?}", self.signature)));
        JsonValue::Array(ret)
    }
}

impl From<BlockInfo> for BlockRecord {
    fn from(block: BlockInfo) -> Self {
        Self {
            header_hash: block.hash().to_string(),
            version: block.header.version,
            previous: block.header.previous.to_string(),
            height: block.header.height,
            timestamp: block.header.timestamp.inner(),
            nonce: block.header.nonce,
            root: block.header.root.to_string(),
            signature: block.signature,
        }
    }
}

impl BlockchainExplorer {
    /// Initialize database with blocks tables.
    pub async fn initialize_blocks(&self) -> WalletDbResult<()> {
        // Initialize blocks database schema
        let database_schema = include_str!("../blocks.sql");
        self.database.exec_batch_sql(database_schema)?;

        Ok(())
    }

    /// Reset blocks table in the database.
    pub fn reset_blocks(&self) -> WalletDbResult<()> {
        info!(target: "blockchain-explorer::blocks::reset_blocks", "Resetting blocks...");
        let query = format!("DELETE FROM {};", BLOCKS_TABLE);
        self.database.exec_sql(&query, &[])
    }

    /// Import given block into the database.
    pub async fn put_block(&self, block: &BlockRecord) -> Result<()> {
        let query = format!(
            "INSERT OR REPLACE INTO {} ({}, {}, {}, {}, {}, {}, {}, {}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);",
            BLOCKS_TABLE,
            BLOCKS_COL_HEADER_HASH,
            BLOCKS_COL_VERSION,
            BLOCKS_COL_PREVIOUS,
            BLOCKS_COL_HEIGHT,
            BLOCKS_COL_TIMESTAMP,
            BLOCKS_COL_NONCE,
            BLOCKS_COL_ROOT,
            BLOCKS_COL_SIGNATURE
        );

        if let Err(e) = self.database.exec_sql(
            &query,
            rusqlite::params![
                block.header_hash,
                block.version,
                block.previous,
                block.height,
                block.timestamp,
                block.nonce,
                block.root,
                serialize(&block.signature),
            ],
        ) {
            return Err(Error::RusqliteError(format!("[put_block] Block insert failed: {e:?}")))
        };

        Ok(())
    }

    /// Auxiliary function to parse a `BLOCKS_TABLE` record.
    fn parse_block_record(&self, row: &[Value]) -> Result<BlockRecord> {
        let Value::Text(ref header_hash) = row[0] else {
            return Err(Error::ParseFailed("[parse_block_record] Header hash parsing failed"))
        };
        let header_hash = header_hash.clone();

        let Value::Integer(version) = row[1] else {
            return Err(Error::ParseFailed("[parse_block_record] Version parsing failed"))
        };
        let Ok(version) = u8::try_from(version) else {
            return Err(Error::ParseFailed("[parse_block_record] Version parsing failed"))
        };

        let Value::Text(ref previous) = row[2] else {
            return Err(Error::ParseFailed("[parse_block_record] Previous parsing failed"))
        };
        let previous = previous.clone();

        let Value::Integer(height) = row[3] else {
            return Err(Error::ParseFailed("[parse_block_record] Height parsing failed"))
        };
        let Ok(height) = u32::try_from(height) else {
            return Err(Error::ParseFailed("[parse_block_record] Height parsing failed"))
        };

        let Value::Integer(timestamp) = row[4] else {
            return Err(Error::ParseFailed("[parse_block_record] Timestamp parsing failed"))
        };
        let Ok(timestamp) = u64::try_from(timestamp) else {
            return Err(Error::ParseFailed("[parse_block_record] Timestamp parsing failed"))
        };

        let Value::Integer(nonce) = row[5] else {
            return Err(Error::ParseFailed("[parse_block_record] Nonce parsing failed"))
        };
        let Ok(nonce) = u64::try_from(nonce) else {
            return Err(Error::ParseFailed("[parse_block_record] Nonce parsing failed"))
        };

        let Value::Text(ref root) = row[6] else {
            return Err(Error::ParseFailed("[parse_block_record] Root parsing failed"))
        };
        let root = root.clone();

        let Value::Blob(ref signature_bytes) = row[7] else {
            return Err(Error::ParseFailed(
                "[parse_block_record] Signature bytes bytes parsing failed",
            ))
        };
        let signature = deserialize(signature_bytes)?;

        Ok(BlockRecord {
            header_hash,
            version,
            previous,
            height,
            timestamp,
            nonce,
            root,
            signature,
        })
    }

    /// Fetch all known blocks from the database.
    pub fn get_blocks(&self) -> Result<Vec<BlockRecord>> {
        let rows = match self.database.query_multiple(BLOCKS_TABLE, &[], &[]) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_blocks] Blocks retrieval failed: {e:?}"
                )))
            }
        };

        let mut blocks = Vec::with_capacity(rows.len());
        for row in rows {
            blocks.push(self.parse_block_record(&row)?);
        }

        Ok(blocks)
    }

    /// Fetch a block given its header hash.
    pub fn get_block_by_hash(&self, header_hash: &str) -> Result<BlockRecord> {
        let row = match self.database.query_single(
            BLOCKS_TABLE,
            &[],
            convert_named_params! {(BLOCKS_COL_HEADER_HASH, header_hash)},
        ) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_block_by_hash] Block retrieval failed: {e:?}"
                )))
            }
        };

        self.parse_block_record(&row)
    }

    /// Fetch last block from the database.
    pub async fn last_block(&self) -> WalletDbResult<u32> {
        // First we prepare the query
        let query = format!(
            "SELECT {} FROM {} ORDER BY {} DESC LIMIT 1;",
            BLOCKS_COL_HEIGHT, BLOCKS_TABLE, BLOCKS_COL_HEIGHT
        );
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
            None => return Ok(0_u32),
        };

        // Parse returned value
        let Ok(value) = row.get(0) else { return Err(WalletDbError::ParseColumnValueError) };
        let Value::Integer(height) = value else {
            return Err(WalletDbError::ParseColumnValueError)
        };
        let Ok(height) = u32::try_from(height) else {
            return Err(WalletDbError::ParseColumnValueError)
        };

        Ok(height)
    }

    /// Auxiliary function to parse a `BLOCKS_TABLE` query rows into block records.
    fn parse_blocks_query_rows(&self, rows: &mut rusqlite::Rows) -> Result<Vec<BlockRecord>> {
        // Loop over returned rows and parse them
        let mut records = vec![];
        loop {
            // Check if an error occured
            let row = match rows.next() {
                Ok(r) => r,
                Err(_) => {
                    return Err(Error::RusqliteError(format!(
                        "[get_last_n_blocks] {}",
                        WalletDbError::QueryExecutionFailed
                    )))
                }
            };

            // Check if no row was returned
            let row = match row {
                Some(r) => r,
                None => break,
            };

            // Grab row returned values
            let mut row_values = vec![];
            let mut idx = 0;
            loop {
                let Ok(value) = row.get(idx) else { break };
                row_values.push(value);
                idx += 1;
            }
            records.push(row_values);
        }

        // Parse the records into blocks
        let mut blocks = Vec::with_capacity(records.len());
        for record in records {
            blocks.push(self.parse_block_record(&record)?);
        }

        Ok(blocks)
    }

    /// Fetch last N blocks from the database.
    pub fn get_last_n_blocks(&self, n: u16) -> Result<Vec<BlockRecord>> {
        // First we prepare the query
        let query = format!(
            "SELECT * FROM {} ORDER BY {} DESC LIMIT {};",
            BLOCKS_TABLE, BLOCKS_COL_HEIGHT, n
        );
        let Ok(conn) = self.database.conn.lock() else {
            return Err(Error::RusqliteError(format!(
                "[get_last_n_blocks] {}",
                WalletDbError::FailedToAquireLock
            )))
        };
        let Ok(mut stmt) = conn.prepare(&query) else {
            return Err(Error::RusqliteError(format!(
                "[get_last_n_blocks] {}",
                WalletDbError::QueryPreparationFailed
            )))
        };

        // Execute the query using provided params
        let Ok(mut rows) = stmt.query([]) else {
            return Err(Error::RusqliteError(format!(
                "[get_last_n_blocks] {}",
                WalletDbError::QueryExecutionFailed
            )))
        };

        self.parse_blocks_query_rows(&mut rows)
    }

    /// Fetch last N blocks from the database.
    pub fn get_blocks_in_heights_range(&self, start: u32, end: u32) -> Result<Vec<BlockRecord>> {
        // First we prepare the query
        let query = format!(
            "SELECT * FROM {} WHERE {} >= {} AND {} <= {} ORDER BY {} ASC;",
            BLOCKS_TABLE, BLOCKS_COL_HEIGHT, start, BLOCKS_COL_HEIGHT, end, BLOCKS_COL_HEIGHT
        );
        let Ok(conn) = self.database.conn.lock() else {
            return Err(Error::RusqliteError(format!(
                "[get_blocks_in_height_range] {}",
                WalletDbError::FailedToAquireLock
            )))
        };
        let Ok(mut stmt) = conn.prepare(&query) else {
            return Err(Error::RusqliteError(format!(
                "[get_blocks_in_height_range] {}",
                WalletDbError::QueryPreparationFailed
            )))
        };

        // Execute the query using provided params
        let Ok(mut rows) = stmt.query([]) else {
            return Err(Error::RusqliteError(format!(
                "[get_blocks_in_height_range] {}",
                WalletDbError::QueryExecutionFailed
            )))
        };

        self.parse_blocks_query_rows(&mut rows)
    }
}
