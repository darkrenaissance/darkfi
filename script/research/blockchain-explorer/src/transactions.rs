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
use tinyjson::JsonValue;

use darkfi::{
    blockchain::{
        HeaderHash, SLED_PENDING_TX_ORDER_TREE, SLED_PENDING_TX_TREE, SLED_TX_LOCATION_TREE,
        SLED_TX_TREE,
    },
    tx::Transaction,
    Error, Result,
};
use darkfi_sdk::tx::TransactionHash;

use crate::ExplorerDb;

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

impl ExplorerDb {
    /// Resets transactions in the database by clearing transaction-related trees, returning an Ok result on success.
    pub fn reset_transactions(&self) -> Result<()> {
        // Initialize transaction trees to reset
        let trees_to_reset =
            [SLED_TX_TREE, SLED_TX_LOCATION_TREE, SLED_PENDING_TX_TREE, SLED_PENDING_TX_ORDER_TREE];

        // Iterate over each associated transaction tree and delete its contents
        for tree_name in &trees_to_reset {
            let tree = &self.blockchain.sled_db.open_tree(tree_name)?;
            tree.clear()?;
            let tree_name_str = std::str::from_utf8(tree_name)?;
            info!(target: "blockchain-explorer::blocks", "Successfully reset transaction tree: {tree_name_str}");
        }

        Ok(())
    }

    /// Provides the transaction count of all the transactions in the explorer database.
    pub fn get_transaction_count(&self) -> usize {
        self.blockchain.txs_len()
    }

    /// Fetch all known transactions from the database.
    pub fn get_transactions(&self) -> Result<Vec<TransactionRecord>> {
        // Retrieve all transactions and handle any errors encountered
        let transactions = self.blockchain.transactions.get_all().map_err(|e| {
            Error::DatabaseError(format!("[get_transactions] Trxs retrieval: {e:?}"))
        })?;

        // Transform the found transactions into a vector of transaction records
        let transaction_records: Vec<TransactionRecord> = transactions
            .iter()
            .map(|(tx_hash, tx)| TransactionRecord::from((&tx_hash.as_string(), tx)))
            .collect();

        Ok(transaction_records)
    }

    /// Fetch all transactions from the database for the given block header hash.
    pub fn get_transactions_by_header_hash(
        &self,
        header_hash: &str,
    ) -> Result<Vec<TransactionRecord>> {
        // Parse header hash, returning an error if parsing fails
        let header_hash = header_hash
            .parse::<HeaderHash>()
            .map_err(|_| Error::ParseFailed("[get_transactions_by_header_hash] Invalid hash"))?;

        // Fetch block by hash and handle encountered errors
        let blocks = match self.blockchain.get_blocks_by_hash(&[header_hash]) {
            Ok(blocks) => blocks,
            Err(Error::BlockNotFound(_)) => return Ok(vec![]),
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[get_transactions_by_header_hash] Block retrieval failed: {e:?}"
                )))
            }
        };

        // Transform block transactions into transaction records
        Ok(blocks[0]
            .txs
            .iter()
            .map(|tx| TransactionRecord::from((&blocks[0].header.hash().as_string(), tx)))
            .collect::<Vec<TransactionRecord>>())
    }

    /// Fetch a transaction given its header hash.
    pub fn get_transaction_by_hash(
        &self,
        tx_hash: &TransactionHash,
    ) -> Result<Option<TransactionRecord>> {
        let tx_store = &self.blockchain.transactions;

        // Attempt to retrieve the transaction using the provided hash handling any potential errors
        let txs = tx_store.get(&[*tx_hash], false).map_err(|e| {
            Error::DatabaseError(format!(
                "[get_transaction_by_hash] Transaction retrieval failed: {e:?}"
            ))
        })?;

        // Check if transaction was found
        if txs[0].is_none() {
            return Ok(None);
        };

        // Retrieve the location of the transaction to obtain its header hash
        let (block_height, _) = tx_store.get_location(&[*tx_hash], true).map_err(|e| {
            Error::DatabaseError(format!(
                "[get_transaction_by_hash] Location retrieval failed: {e:?}"
            ))
        })?[0]
            .unwrap();

        // Retrieve the block corresponding to the transaction's height
        let header_hash =
            &self.blockchain.blocks.get_order(&[block_height], true).map_err(|e| {
                Error::DatabaseError(format!(
                    "[get_transaction_by_hash] Block retrieval failed: {e:?}"
                ))
            })?[0]
                .unwrap();

        // Transform the transaction into a TransactionRecord
        Ok(Some(TransactionRecord::from((&header_hash.as_string(), txs[0].as_ref().unwrap()))))
    }
}
