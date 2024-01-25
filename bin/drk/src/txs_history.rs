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

use darkfi::{tx::Transaction, Error, Result};
use darkfi_serial::{deserialize, serialize};

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
    /// Insert a [`Transaction`] history record into the wallet.
    pub async fn insert_tx_history_record(&self, tx: &Transaction) -> WalletDbResult<()> {
        let query = format!(
            "INSERT INTO {} ({}, {}, {}) VALUES (?1, ?2, ?3);",
            WALLET_TXS_HISTORY_TABLE,
            WALLET_TXS_HISTORY_COL_TX_HASH,
            WALLET_TXS_HISTORY_COL_STATUS,
            WALLET_TXS_HISTORY_COL_TX,
        );
        let Ok(tx_hash) = tx.hash() else { return Err(WalletDbError::QueryPreparationFailed) };
        self.wallet
            .exec_sql(
                &query,
                rusqlite::params![
                    tx_hash.to_string(),
                    "Broadcasted",
                    bs58::encode(&serialize(tx)).into_string()
                ],
            )
            .await
    }

    /// Get a transaction history record.
    pub async fn get_tx_history_record(
        &self,
        tx_hash: &str,
    ) -> Result<(String, String, Transaction)> {
        let row = match self
            .wallet
            .query_single(
                WALLET_TXS_HISTORY_TABLE,
                &[],
                convert_named_params! {(WALLET_TXS_HISTORY_COL_TX_HASH, tx_hash)},
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(Error::RusqliteError(format!(
                    "[get_tx_history_record] Transaction history record retrieval failed: {e:?}"
                )))
            }
        };

        let Value::Text(ref tx_hash) = row[0] else {
            return Err(Error::ParseFailed(
                "[get_tx_history_record] Transaction hash parsing failed",
            ))
        };
        let tx_hash = tx_hash.clone();

        let Value::Text(ref status) = row[1] else {
            return Err(Error::ParseFailed("[get_tx_history_record] Status parsing failed"))
        };
        let status = status.clone();

        let Value::Text(ref tx_encoded) = row[2] else {
            return Err(Error::ParseFailed(
                "[get_tx_history_record] Encoded transaction parsing failed",
            ))
        };
        let tx_bytes: Vec<u8> = bs58::decode(tx_encoded).into_vec()?;
        let tx: Transaction = deserialize(&tx_bytes)?;

        Ok((tx_hash, status, tx))
    }

    /// Fetch all transactions history records, excluding bytes column.
    pub async fn get_txs_history(&self) -> WalletDbResult<Vec<(String, String)>> {
        let rows = self
            .wallet
            .query_multiple(
                WALLET_TXS_HISTORY_TABLE,
                &[WALLET_TXS_HISTORY_COL_TX_HASH, WALLET_TXS_HISTORY_COL_STATUS],
                &[],
            )
            .await?;

        let mut ret = Vec::with_capacity(rows.len());
        for row in rows {
            let Value::Text(ref tx_hash) = row[0] else {
                return Err(WalletDbError::ParseColumnValueError)
            };
            let tx_hash = tx_hash.clone();

            let Value::Text(ref status) = row[1] else {
                return Err(WalletDbError::ParseColumnValueError)
            };
            let status = status.clone();

            ret.push((tx_hash, status));
        }

        Ok(ret)
    }

    /// Update a transactions history record status to the given one.
    pub async fn update_tx_history_record_status(
        &self,
        tx_hash: &str,
        status: &str,
    ) -> WalletDbResult<()> {
        let query = format!(
            "UPDATE {} SET {} = ?1 WHERE {} = ?2;",
            WALLET_TXS_HISTORY_TABLE, WALLET_TXS_HISTORY_COL_STATUS, WALLET_TXS_HISTORY_COL_TX_HASH,
        );
        self.wallet.exec_sql(&query, rusqlite::params![status, tx_hash]).await
    }

    /// Update given transactions history record statuses to the given one.
    pub async fn update_tx_history_records_status(
        &self,
        txs: &Vec<Transaction>,
        status: &str,
    ) -> WalletDbResult<()> {
        if txs.is_empty() {
            return Ok(())
        }

        let mut txs_hashes = Vec::with_capacity(txs.len());
        for tx in txs {
            let Ok(tx_hash) = tx.hash() else { return Err(WalletDbError::QueryPreparationFailed) };
            txs_hashes.push(tx_hash);
        }
        let txs_hashes_string = format!("{:?}", txs_hashes).replace('[', "(").replace(']', ")");
        let query = format!(
            "UPDATE {} SET {} = ?1 WHERE {} IN {};",
            WALLET_TXS_HISTORY_TABLE,
            WALLET_TXS_HISTORY_COL_STATUS,
            WALLET_TXS_HISTORY_COL_TX_HASH,
            txs_hashes_string
        );

        self.wallet.exec_sql(&query, rusqlite::params![status]).await
    }

    /// Update all transaction history records statuses to the given one.
    pub async fn update_all_tx_history_records_status(&self, status: &str) -> WalletDbResult<()> {
        let query = format!(
            "UPDATE {} SET {} = ?1",
            WALLET_TXS_HISTORY_TABLE, WALLET_TXS_HISTORY_COL_STATUS,
        );
        self.wallet.exec_sql(&query, rusqlite::params![status]).await
    }
}
