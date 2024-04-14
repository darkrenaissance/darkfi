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

use lazy_static::lazy_static;
use rusqlite::types::Value;

use darkfi::{tx::Transaction, util::encoding::base64, Error, Result};
use darkfi_sdk::crypto::MONEY_CONTRACT_ID;
use darkfi_serial::{deserialize_async, serialize_async};

use crate::{
    convert_named_params,
    error::{WalletDbError, WalletDbResult},
    Drk,
};

// Wallet SQL table constant names. These have to represent the `wallet.sql`
// SQL schema. Table names are prefixed with the contract ID to avoid collisions.
lazy_static! {
    pub static ref WALLET_TXS_HISTORY_TABLE: String =
        format!("{}_transactions_history", MONEY_CONTRACT_ID.to_string());
}
const WALLET_TXS_HISTORY_COL_TX_HASH: &str = "transaction_hash";
const WALLET_TXS_HISTORY_COL_STATUS: &str = "status";
const WALLET_TXS_HISTORY_COL_TX: &str = "tx";

impl Drk {
    /// Insert a [`Transaction`] history record into the wallet.
    pub async fn insert_tx_history_record(&self, tx: &Transaction) -> WalletDbResult<()> {
        let query = format!(
            "INSERT INTO {} ({}, {}, {}) VALUES (?1, ?2, ?3);",
            *WALLET_TXS_HISTORY_TABLE,
            WALLET_TXS_HISTORY_COL_TX_HASH,
            WALLET_TXS_HISTORY_COL_STATUS,
            WALLET_TXS_HISTORY_COL_TX,
        );
        let tx_hash = tx.hash();
        self.wallet
            .exec_sql(
                &query,
                rusqlite::params![
                    tx_hash.to_string(),
                    "Broadcasted",
                    base64::encode(&serialize_async(tx).await),
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
                &WALLET_TXS_HISTORY_TABLE,
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

        let Some(tx_bytes) = base64::decode(tx_encoded) else {
            return Err(Error::ParseFailed(
                "[get_tx_history_record] Encoded transaction parsing failed",
            ))
        };

        let tx: Transaction = deserialize_async(&tx_bytes).await?;

        Ok((tx_hash, status, tx))
    }

    /// Fetch all transactions history records, excluding bytes column.
    pub async fn get_txs_history(&self) -> WalletDbResult<Vec<(String, String)>> {
        let rows = self
            .wallet
            .query_multiple(
                &WALLET_TXS_HISTORY_TABLE,
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
            *WALLET_TXS_HISTORY_TABLE,
            WALLET_TXS_HISTORY_COL_STATUS,
            WALLET_TXS_HISTORY_COL_TX_HASH,
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
            let tx_hash = tx.hash();
            txs_hashes.push(format!("{tx_hash}"));
        }
        let txs_hashes_string = format!("{:?}", txs_hashes).replace('[', "(").replace(']', ")");
        let query = format!(
            "UPDATE {} SET {} = ?1 WHERE {} IN {};",
            *WALLET_TXS_HISTORY_TABLE,
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
            *WALLET_TXS_HISTORY_TABLE, WALLET_TXS_HISTORY_COL_STATUS,
        );
        self.wallet.exec_sql(&query, rusqlite::params![status]).await
    }
}
