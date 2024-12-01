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
        BlockInfo, HeaderHash, SLED_PENDING_TX_ORDER_TREE, SLED_PENDING_TX_TREE,
        SLED_TX_LOCATION_TREE, SLED_TX_TREE,
    },
    tx::Transaction,
    util::time::Timestamp,
    validator::fees::GasData,
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
    /// Time transaction was added to the block
    pub timestamp: Timestamp,
    /// Total gas used for processing transaction
    pub total_gas_used: u64,
    /// Gas used by WASM
    pub wasm_gas_used: u64,
    /// Gas used by ZK circuit operations
    pub zk_circuit_gas_used: u64,
    /// Gas used for creating the transaction signature
    pub signature_gas_used: u64,
    /// Gas used for deployments
    pub deployment_gas_used: u64,
}

impl TransactionRecord {
    /// Auxiliary function to convert a `TransactionRecord` into a `JsonValue` array.
    pub fn to_json_array(&self) -> JsonValue {
        JsonValue::Array(vec![
            JsonValue::String(self.transaction_hash.clone()),
            JsonValue::String(self.header_hash.clone()),
            JsonValue::String(format!("{:?}", self.payload)),
            JsonValue::String(self.timestamp.to_string()),
            JsonValue::Number(self.total_gas_used as f64),
            JsonValue::Number(self.wasm_gas_used as f64),
            JsonValue::Number(self.zk_circuit_gas_used as f64),
            JsonValue::Number(self.signature_gas_used as f64),
            JsonValue::Number(self.deployment_gas_used as f64),
        ])
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

    /// Fetches all known transactions from the database.
    ///
    /// This function retrieves all transactions stored in the database and transforms
    /// them into a vector of [`TransactionRecord`]s. If no transactions are found,
    /// it returns an empty vector.
    pub fn get_transactions(&self) -> Result<Vec<TransactionRecord>> {
        // Retrieve all transactions and handle any errors encountered
        let txs = self.blockchain.transactions.get_all().map_err(|e| {
            Error::DatabaseError(format!("[get_transactions] Trxs retrieval: {e:?}"))
        })?;

        // Transform the found `Transactions` into a vector of `TransactionRecords`
        let txs_records = txs
            .iter()
            .map(|(_, tx)| self.to_tx_record(None, tx))
            .collect::<Result<Vec<TransactionRecord>>>()?;

        Ok(txs_records)
    }

    /// Fetches all transactions from the database for the given block `header_hash`.
    ///
    /// This function retrieves all transactions associated with the specified
    /// block header hash. It first parses the header hash and then fetches
    /// the corresponding [`BlockInfo`]. If the block is found, it transforms its
    /// transactions into a vector of [`TransactionRecord`]s. If no transactions
    /// are found, it returns an empty vector.
    pub fn get_transactions_by_header_hash(
        &self,
        header_hash: &str,
    ) -> Result<Vec<TransactionRecord>> {
        // Parse header hash, returning an error if parsing fails
        let header_hash = header_hash
            .parse::<HeaderHash>()
            .map_err(|_| Error::ParseFailed("[get_transactions_by_header_hash] Invalid hash"))?;

        // Fetch block by hash and handle encountered errors
        let block = match self.blockchain.get_blocks_by_hash(&[header_hash]) {
            Ok(blocks) => blocks.first().cloned().unwrap(),
            Err(Error::BlockNotFound(_)) => return Ok(vec![]),
            Err(e) => {
                return Err(Error::DatabaseError(format!(
                    "[get_transactions_by_header_hash] Block retrieval failed: {e:?}"
                )))
            }
        };

        // Transform block transactions into transaction records
        block
            .txs
            .iter()
            .map(|tx| self.to_tx_record(self.get_block_info(block.header.hash())?, tx))
            .collect::<Result<Vec<TransactionRecord>>>()
    }

    /// Fetches a transaction given its header hash.
    ///
    /// This function retrieves the transaction associated with the provided
    /// [`TransactionHash`] and transforms it into a [`TransactionRecord`] if found.
    /// If no transaction is found, it returns `None`.
    pub fn get_transaction_by_hash(
        &self,
        tx_hash: &TransactionHash,
    ) -> Result<Option<TransactionRecord>> {
        let tx_store = &self.blockchain.transactions;

        // Attempt to retrieve the transaction using the provided hash handling any potential errors
        let tx_opt = &tx_store.get(&[*tx_hash], false).map_err(|e| {
            Error::DatabaseError(format!(
                "[get_transaction_by_hash] Transaction retrieval failed: {e:?}"
            ))
        })?[0];

        // Transform `Transaction` to a `TransactionRecord`, returning None if no transaction was found
        tx_opt.as_ref().map(|tx| self.to_tx_record(None, tx)).transpose()
    }

    /// Fetches the [`BlockInfo`] associated with a given transaction hash.
    ///
    /// This auxiliary function first fetches the location of the transaction in the blockchain.
    /// If the location is found, it retrieves the associated [`HeaderHash`] and then fetches
    /// the block information corresponding to that header hash. The function returns the
    /// [`BlockInfo`] if successful, or `None` if no location or header hash is found.
    fn get_tx_block_info(&self, tx_hash: &TransactionHash) -> Result<Option<BlockInfo>> {
        // Retrieve the location of the transaction
        let location =
            self.blockchain.transactions.get_location(&[*tx_hash], false).map_err(|e| {
                Error::DatabaseError(format!(
                    "[get_tx_block_info] Location retrieval failed: {e:?}"
                ))
            })?[0];

        // Fetch the `HeaderHash` associated with the location
        let header_hash = match location {
            None => return Ok(None),
            Some((block_height, _)) => {
                self.blockchain.blocks.get_order(&[block_height], false).map_err(|e| {
                    Error::DatabaseError(format!(
                        "[get_tx_block_info] Block retrieval failed: {e:?}"
                    ))
                })?[0]
            }
        };

        // Return the associated `BlockInfo` if the header hash is found; otherwise, return `None`.
        match header_hash {
            None => Ok(None),
            Some(header_hash) => self.get_block_info(header_hash).map_err(|e| {
                Error::DatabaseError(format!(
                    "[get_tx_block_info] BlockInfo retrieval failed: {e:?}"
                ))
            }),
        }
    }

    /// Fetches the [`BlockInfo`] associated with a given [`HeaderHash`].
    ///
    /// This auxiliary function attempts to retrieve the block information using
    /// the specified [`HeaderHash`]. It returns the associated [`BlockInfo`] if found,
    /// or `None` when not found.
    fn get_block_info(&self, header_hash: HeaderHash) -> Result<Option<BlockInfo>> {
        match self.blockchain.get_blocks_by_hash(&[header_hash]) {
            Err(Error::BlockNotFound(_)) => Ok(None),
            Ok(block_info) => Ok(block_info.into_iter().next()),
            Err(e) => Err(Error::DatabaseError(format!(
                "[get_transactions_by_header_hash] Block retrieval failed: {e:?}"
            ))),
        }
    }

    /// Converts a [`Transaction`] and its associated block information into a [`TransactionRecord`].
    ///
    /// This auxiliary function first retrieves the gas data associated with the provided transaction.
    /// If [`BlockInfo`] is not provided, it attempts to fetch it using the transaction's hash,
    /// returning an error if the block information cannot be found. Upon success, the function
    /// returns a [`TransactionRecord`] containing relevant details about the transaction.
    fn to_tx_record(
        &self,
        block_info_opt: Option<BlockInfo>,
        tx: &Transaction,
    ) -> Result<TransactionRecord> {
        // Fetch the gas data associated with the transaction
        let gas_data_option = self.metrics_store.get_tx_gas_data(&tx.hash()).map_err(|e| {
            Error::DatabaseError(format!(
                "[to_tx_record] Failed to fetch the gas data associated with transaction {}: {e:?}",
                tx.hash()
            ))
        })?;

        // Unwrap the option, providing a default value when `None`
        let gas_data = gas_data_option.unwrap_or_else(GasData::default);

        // Process provided block_info option
        let block_info = match block_info_opt {
            // Use provided block_info when present
            Some(block_info) => block_info,
            // Fetch the block info associated with the transaction when block info not provided
            None => {
                match self.get_tx_block_info(&tx.hash())? {
                    Some(block_info) => block_info,
                    // If no associated block info found, throw an error as this should not happen
                    None => {
                        return Err(Error::BlockNotFound(format!(
                            "[to_tx_record] Required `BlockInfo` was not found for transaction: {}",
                            tx.hash()
                        )))
                    }
                }
            }
        };

        // Return transformed transaction record
        Ok(TransactionRecord {
            transaction_hash: tx.hash().to_string(),
            header_hash: block_info.hash().to_string(),
            timestamp: block_info.header.timestamp,
            payload: tx.clone(),
            total_gas_used: gas_data.total_gas_used(),
            wasm_gas_used: gas_data.wasm,
            zk_circuit_gas_used: gas_data.zk_circuits,
            signature_gas_used: gas_data.signatures,
            deployment_gas_used: gas_data.deployments,
        })
    }
}
