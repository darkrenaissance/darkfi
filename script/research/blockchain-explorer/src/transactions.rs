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

use darkfi::{tx::Transaction, Error, Result};
use darkfi_serial::{deserialize, serialize};
use drk::{convert_named_params, error::WalletDbResult};

use crate::BlockchainExplorer;

// Database SQL table constant names. These have to represent the `transactions.sql`
// SQL schema.
pub const TRANSACTIONS_TABLE: &str = "transactions";

// TRANSACTIONS_TABLE
pub const TRANSACTIONS_COL_TRANSACTION_HASH: &str = "transaction_hash";
pub const TRANSACTIONS_COL_HEADER_HASH: &str = "header_hash";
pub const TRANSACTIONS_COL_PAYLOAD: &str = "payload";

#[derive(Debug, Clone)]
/// Structure representing a `TRANSACTIONS_TABLE` record.
pub struct TransactionRecord {
    /// Transaction hash identifier
    pub transaction_hash: String,
    /// Header hash identifier of the block this transaction was included in
    pub header_hash: String,
    // TODO: Split the payload into a more easily readable fields
    /// Transaction payload
    pub payload: Transaction,
}

impl TransactionRecord {
    /// Auxiliary function to convert a `TransactionRecord` into a `JsonValue` array.
    pub fn to_json_array(&self) -> JsonValue {
        JsonValue::Array(vec![
            JsonValue::String(self.transaction_hash.clone()),
            JsonValue::String(self.header_hash.clone()),
            JsonValue::String(format!("{:?}", self.payload)),
        ])
    }
}

impl From<(&String, &Transaction)> for TransactionRecord {
    fn from((header_hash, transaction): (&String, &Transaction)) -> Self {
        Self {
            transaction_hash: transaction.hash().to_string(),
            header_hash: header_hash.clone(),
            payload: transaction.clone(),
        }
    }
}

impl BlockchainExplorer {
    /// Initialize database with transactions tables.
    pub async fn initialize_transactions(&self) -> WalletDbResult<()> {
        // Initialize transactions database schema
        let database_schema = include_str!("../transactions.sql");
        self.database.exec_batch_sql(database_schema)?;

        Ok(())
    }

    /// Reset transactions table in the database.
    pub fn reset_transactions(&self) -> WalletDbResult<()> {
        info!(target: "blockchain-explorer::transactions::reset_transactions", "Resetting transactions...");
        let query = format!("DELETE FROM {};", TRANSACTIONS_TABLE);
        self.database.exec_sql(&query, &[])
    }

    /// Import given transaction into the database.
    pub async fn put_transaction(&self, transaction: &TransactionRecord) -> Result<()> {
        let query = format!(
            "INSERT OR REPLACE INTO {} ({}, {}, {}) VALUES (?1, ?2, ?3);",
            TRANSACTIONS_TABLE,
            TRANSACTIONS_COL_TRANSACTION_HASH,
            TRANSACTIONS_COL_HEADER_HASH,
            TRANSACTIONS_COL_PAYLOAD
        );

        if let Err(e) = self.database.exec_sql(
            &query,
            rusqlite::params![
                transaction.transaction_hash,
                transaction.header_hash,
                serialize(&transaction.payload),
            ],
        ) {
            return Err(Error::RusqliteError(format!(
                "[put_transaction] Transaction insert failed: {e:?}"
            )))
        };

        Ok(())
    }

    /// Auxiliary function to parse a `TRANSACTIONS_TABLE` record.
    fn parse_transaction_record(&self, row: &[Value]) -> Result<TransactionRecord> {
        let Value::Text(ref transaction_hash) = row[0] else {
            return Err(Error::ParseFailed(
                "[parse_transaction_record] Transaction hash parsing failed",
            ))
        };
        let transaction_hash = transaction_hash.clone();

        let Value::Text(ref header_hash) = row[1] else {
            return Err(Error::ParseFailed("[parse_transaction_record] Header hash parsing failed"))
        };
        let header_hash = header_hash.clone();

        let Value::Blob(ref payload_bytes) = row[2] else {
            return Err(Error::ParseFailed(
                "[parse_transaction_record] Payload bytes bytes parsing failed",
            ))
        };
        let payload = deserialize(payload_bytes)?;

        Ok(TransactionRecord { transaction_hash, header_hash, payload })
    }

    /// Fetch all known transactions from the database.
    pub fn get_transactions(&self) -> Result<Vec<TransactionRecord>> {
        let rows = match self.database.query_multiple(TRANSACTIONS_TABLE, &[], &[]) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_transactions] Transactions retrieval failed: {e:?}"
                )))
            }
        };

        let mut transactions = Vec::with_capacity(rows.len());
        for row in rows {
            transactions.push(self.parse_transaction_record(&row)?);
        }

        Ok(transactions)
    }

    /// Fetch all transactions from the database for the given block header hash.
    pub fn get_transactions_by_header_hash(
        &self,
        header_hash: &str,
    ) -> Result<Vec<TransactionRecord>> {
        let rows = match self.database.query_multiple(
            TRANSACTIONS_TABLE,
            &[],
            convert_named_params! {(TRANSACTIONS_COL_HEADER_HASH, header_hash)},
        ) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_transactions_by_header_hash] Transactions retrieval failed: {e:?}"
                )))
            }
        };

        let mut transactions = Vec::with_capacity(rows.len());
        for row in rows {
            transactions.push(self.parse_transaction_record(&row)?);
        }

        Ok(transactions)
    }

    /// Fetch a transaction given its header hash.
    pub fn get_transaction_by_hash(&self, transaction_hash: &str) -> Result<TransactionRecord> {
        let row = match self.database.query_single(
            TRANSACTIONS_TABLE,
            &[],
            convert_named_params! {(TRANSACTIONS_COL_TRANSACTION_HASH, transaction_hash)},
        ) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_transaction_by_hash] Transaction retrieval failed: {e:?}"
                )))
            }
        };

        self.parse_transaction_record(&row)
    }
}
