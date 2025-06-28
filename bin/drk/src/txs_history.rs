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

use darkfi::{tx::Transaction, Error, Result};
use darkfi_serial::{deserialize_async, serialize_async};

use crate::{
    convert_named_params,
    error::{WalletDbError, WalletDbResult},
    Drk,
};

// Wallet SQL table constant names. These have to represent the `wallet.sql`
// SQL schema.
const WALLET_TXS_HISTORY_TABLE: &str = "transactions_history";
const WALLET_TXS_HISTORY_COL_TX_HASH: &str = "transaction_hash";
const WALLET_TXS_HISTORY_COL_STATUS: &str = "status";
const WALLET_TXS_HISTORY_COL_TX: &str = "tx";

impl Drk {
    /// Insert or update a `Transaction` history record into the wallet,
    /// with the provided status, and store its inverse query into the cache.
    pub async fn put_tx_history_record(
        &self,
        tx: &Transaction,
        status: &str,
    ) -> WalletDbResult<String> {
        // Create an SQL `INSERT OR REPLACE` query
        let query = format!(
            "INSERT OR REPLACE INTO {WALLET_TXS_HISTORY_TABLE} ({WALLET_TXS_HISTORY_COL_TX_HASH}, {WALLET_TXS_HISTORY_COL_STATUS}, {WALLET_TXS_HISTORY_COL_TX}) VALUES (?1, ?2, ?3);"
        );

        // Create its inverse query
        let tx_hash = tx.hash().to_string();
        // We only need to set the transaction status to "Reverted"
        let inverse = self.wallet.create_prepared_statement(
            &format!(
                "UPDATE {WALLET_TXS_HISTORY_TABLE} SET {WALLET_TXS_HISTORY_COL_STATUS} = ?1 WHERE {WALLET_TXS_HISTORY_COL_TX_HASH} = ?2;"
            ),
            rusqlite::params!["Reverted", tx_hash],
        )?;

        // Execute the query
        self.wallet
            .exec_sql(&query, rusqlite::params![tx_hash, status, &serialize_async(tx).await,])?;

        // Store its inverse
        self.wallet.cache_inverse(inverse)?;

        Ok(tx_hash)
    }

    /// Insert or update a slice of [`Transaction`] history records into the wallet,
    /// with the provided status.
    pub async fn put_tx_history_records(
        &self,
        txs: &[&Transaction],
        status: &str,
    ) -> WalletDbResult<Vec<String>> {
        let mut ret = Vec::with_capacity(txs.len());
        for tx in txs {
            ret.push(self.put_tx_history_record(tx, status).await?);
        }
        Ok(ret)
    }

    /// Get a transaction history record.
    pub async fn get_tx_history_record(
        &self,
        tx_hash: &str,
    ) -> Result<(String, String, Transaction)> {
        let row = match self.wallet.query_single(
            WALLET_TXS_HISTORY_TABLE,
            &[],
            convert_named_params! {(WALLET_TXS_HISTORY_COL_TX_HASH, tx_hash)},
        ) {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[get_tx_history_record] Transaction history record retrieval failed: {e:?}"
                )))
            }
        };

        let Value::Text(ref tx_hash) = row[0] else {
            return Err(Error::ParseFailed(
                "[get_tx_history_record] Transaction hash parsing failed",
            ))
        };

        let Value::Text(ref status) = row[1] else {
            return Err(Error::ParseFailed("[get_tx_history_record] Status parsing failed"))
        };

        let Value::Blob(ref bytes) = row[2] else {
            return Err(Error::ParseFailed(
                "[get_tx_history_record] Transaction bytes parsing failed",
            ))
        };
        let tx: Transaction = deserialize_async(bytes).await?;

        Ok((tx_hash.clone(), status.clone(), tx))
    }

    /// Fetch all transactions history records, excluding bytes column.
    pub fn get_txs_history(&self) -> WalletDbResult<Vec<(String, String)>> {
        let rows = self.wallet.query_multiple(
            WALLET_TXS_HISTORY_TABLE,
            &[WALLET_TXS_HISTORY_COL_TX_HASH, WALLET_TXS_HISTORY_COL_STATUS],
            &[],
        )?;

        let mut ret = Vec::with_capacity(rows.len());
        for row in rows {
            let Value::Text(ref tx_hash) = row[0] else {
                return Err(WalletDbError::ParseColumnValueError)
            };

            let Value::Text(ref status) = row[1] else {
                return Err(WalletDbError::ParseColumnValueError)
            };

            ret.push((tx_hash.clone(), status.clone()));
        }

        Ok(ret)
    }

    /// Reset the transaction history records in the wallet.
    pub fn reset_tx_history(&self) -> WalletDbResult<()> {
        println!("Resetting transactions history");
        let query = format!("DELETE FROM {WALLET_TXS_HISTORY_TABLE};");
        self.wallet.exec_sql(&query, &[])?;
        println!("Successfully reset transactions history");

        Ok(())
    }

    /// Remove the transaction history records in the wallet
    /// that have been reverted.
    pub fn remove_reverted_txs(&self) -> WalletDbResult<()> {
        println!("Removing reverted transactions history records");
        let query = format!(
            "DELETE FROM {WALLET_TXS_HISTORY_TABLE} WHERE {WALLET_TXS_HISTORY_COL_STATUS} = 'Reverted';"
        );
        self.wallet.exec_sql(&query, &[])?;
        println!("Successfully removed reverted transactions history records");

        Ok(())
    }
}
