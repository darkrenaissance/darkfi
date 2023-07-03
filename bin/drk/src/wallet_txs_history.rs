/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use anyhow::{anyhow, Result};
use darkfi::{rpc::jsonrpc::JsonRequest, tx::Transaction, wallet::walletdb::QueryType};
use darkfi_serial::{deserialize, serialize};
use serde_json::json;

use super::Drk;

// Wallet SQL table constant names. These have to represent the `wallet.sql`
// SQL schema.
const WALLET_TXS_HISTORY_TABLE: &str = "transactions_history";
const WALLET_TXS_HISTORY_COL_TX_HASH: &str = "transaction_hash";
const WALLET_TXS_HISTORY_COL_STATUS: &str = "status";
const WALLET_TXS_HISTORY_COL_TX: &str = "tx";

impl Drk {
    /// Fetch all transactions history records, excluding bytes column.
    pub async fn get_txs_history(&self) -> Result<Vec<(String, String)>> {
        let mut ret = vec![];

        let query = format!(
            "SELECT {}, {} FROM {};",
            WALLET_TXS_HISTORY_COL_TX_HASH, WALLET_TXS_HISTORY_COL_STATUS, WALLET_TXS_HISTORY_TABLE
        );

        let params = json!([
            query,
            QueryType::Text as u8,
            WALLET_TXS_HISTORY_COL_TX_HASH,
            QueryType::Text as u8,
            WALLET_TXS_HISTORY_COL_STATUS,
        ]);

        let req = JsonRequest::new("wallet.query_row_multi", params);
        let rep = self.rpc_client.request(req).await?;

        let Some(rows) = rep.as_array() else {
            return Err(anyhow!("[txs_history] Unexpected response from darkfid: {}", rep))
        };

        for row in rows {
            let tx_hash: String = serde_json::from_value(row[0].clone())?;
            let status: String = serde_json::from_value(row[1].clone())?;
            ret.push((tx_hash, status));
        }

        Ok(ret)
    }

    /// Get a transaction history record.
    pub async fn get_tx_history_record(
        &self,
        tx_hash: &str,
    ) -> Result<(String, String, Transaction)> {
        let query = format!(
            "SELECT * FROM {} WHERE {} = {};",
            WALLET_TXS_HISTORY_TABLE, WALLET_TXS_HISTORY_COL_TX_HASH, tx_hash
        );

        let params = json!([
            query,
            QueryType::Text as u8,
            WALLET_TXS_HISTORY_COL_TX_HASH,
            QueryType::Text as u8,
            WALLET_TXS_HISTORY_COL_STATUS,
            QueryType::Text as u8,
            WALLET_TXS_HISTORY_COL_TX,
        ]);

        let req = JsonRequest::new("wallet.query_row_single", params);
        let rep = self.rpc_client.request(req).await?;

        let Some(arr) = rep.as_array() else {
            return Err(anyhow!("[get_tx_history_record] Unexpected response from darkfid: {}", rep))
        };

        if arr.len() != 3 {
            return Err(anyhow!("Did not find transaction record with hash {}", tx_hash))
        }

        let tx_hash: String = serde_json::from_value(arr[0].clone())?;

        let status: String = serde_json::from_value(arr[1].clone())?;

        let tx_encoded: String = serde_json::from_value(arr[2].clone())?;
        let tx_bytes: Vec<u8> = bs58::decode(&tx_encoded).into_vec()?;
        let tx: Transaction = deserialize(&tx_bytes)?;

        Ok((tx_hash, status, tx))
    }

    /// Insert a [`Transaction`] history record into the wallet.
    pub async fn insert_tx_history_record(&self, tx: &Transaction) -> Result<()> {
        let query = format!(
            "INSERT INTO {} ({}, {}, {}) VALUES (?1, ?2, ?3);",
            WALLET_TXS_HISTORY_TABLE,
            WALLET_TXS_HISTORY_COL_TX_HASH,
            WALLET_TXS_HISTORY_COL_STATUS,
            WALLET_TXS_HISTORY_COL_TX,
        );

        let params = json!([
            query,
            QueryType::Text as u8,
            tx.hash().to_string(),
            QueryType::Text as u8,
            "Broadcasted",
            QueryType::Text as u8,
            bs58::encode(&serialize(tx)).into_string(),
        ]);

        let req = JsonRequest::new("wallet.exec_sql", params);
        let _ = self.rpc_client.request(req).await?;

        Ok(())
    }

    /// Update a transactions history record status to the given one.
    pub async fn update_tx_history_record_status(&self, tx_hash: &str, status: &str) -> Result<()> {
        let query = format!(
            "UPDATE {} SET {} = ?1 WHERE {} = ?2;",
            WALLET_TXS_HISTORY_TABLE, WALLET_TXS_HISTORY_COL_STATUS, WALLET_TXS_HISTORY_COL_TX_HASH,
        );

        let params = json!([query, QueryType::Text as u8, status, QueryType::Text as u8, tx_hash,]);

        let req = JsonRequest::new("wallet.exec_sql", params);
        let _ = self.rpc_client.request(req).await?;

        Ok(())
    }

    /// Update given transactions history record statuses to the given one.
    pub async fn update_tx_history_records_status(
        &self,
        txs: &Vec<Transaction>,
        status: &str,
    ) -> Result<()> {
        if txs.is_empty() {
            return Ok(())
        }

        let txs_hashes: Vec<String> = txs.iter().map(|tx| tx.hash().to_string()).collect();
        let txs_hashes_string = format!("{:?}", txs_hashes).replace('[', "(").replace(']', ")");
        let query = format!(
            "UPDATE {} SET {} = ?1 WHERE {} IN {};",
            WALLET_TXS_HISTORY_TABLE,
            WALLET_TXS_HISTORY_COL_STATUS,
            WALLET_TXS_HISTORY_COL_TX_HASH,
            txs_hashes_string
        );

        let params = json!([query, QueryType::Text as u8, status,]);

        let req = JsonRequest::new("wallet.exec_sql", params);
        let _ = self.rpc_client.request(req).await?;

        Ok(())
    }

    /// Update all transaction history records statuses to the given one.
    pub async fn update_all_tx_history_records_status(&self, status: &str) -> Result<()> {
        let query = format!(
            "UPDATE {} SET {} = ?1",
            WALLET_TXS_HISTORY_TABLE, WALLET_TXS_HISTORY_COL_STATUS,
        );

        let params = json!([query, QueryType::Text as u8, status,]);

        let req = JsonRequest::new("wallet.exec_sql", params);
        let _ = self.rpc_client.request(req).await?;

        Ok(())
    }
}
