/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use tinyjson::JsonValue;

use darkfi::{rpc::jsonrpc::parse_json_array_string, Result};
use darkfi_sdk::tx::TransactionHash;

use crate::{error::ExplorerdError, Explorerd};

impl Explorerd {
    // RPCAPI:
    // Queries the database to retrieve the transactions corresponding to the provided block header hash.
    // Returns the readable transactions upon success.
    //
    // **Params:**
    // * `array[0]`: `String` Block header hash
    //
    // **Returns:**
    // * Array of `TransactionRecord` encoded into a JSON.
    //
    // **Example API Usage:**
    // --> {"jsonrpc": "2.0", "method": "transactions.get_transactions_by_header_hash", "params": ["5cc...2f9"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn transactions_get_transactions_by_header_hash(
        &self,
        params: &JsonValue,
    ) -> Result<JsonValue> {
        // Extract header hash
        let header_hash = parse_json_array_string("header_hash", 0, params)?;

        // Retrieve transactions by header hash
        let transactions = self.service.get_transactions_by_header_hash(&header_hash)?;

        // Convert transactions into a JSON array, return result
        Ok(JsonValue::Array(transactions.iter().map(|tx| tx.to_json_array()).collect()))
    }

    // RPCAPI:
    // Queries the database to retrieve the transaction corresponding to the provided hash.
    // Returns the readable transaction upon success.
    //
    // **Params:**
    // * `array[0]`: `String` Transaction hash
    //
    // **Returns:**
    // * `TransactionRecord` encoded into a JSON.
    //
    // **Example API Usage:**
    // --> {"jsonrpc": "2.0", "method": "transactions.get_transaction_by_hash", "params": ["7e7...b4d"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn transactions_get_transaction_by_hash(
        &self,
        params: &JsonValue,
    ) -> Result<JsonValue> {
        // Extract transaction hash
        let tx_hash_str = parse_json_array_string("tx_hash", 0, params)?;

        // Convert the provided hash into a `TransactionHash` instance
        let tx_hash = tx_hash_str
            .parse::<TransactionHash>()
            .map_err(|_| ExplorerdError::InvalidTxHash(tx_hash_str.to_string()))?;

        // Retrieve the transaction by its hash, returning the result as a JsonValue array
        match self.service.get_transaction_by_hash(&tx_hash)? {
            Some(transaction) => Ok(transaction.to_json_array()),
            None => Ok(JsonValue::Array(vec![])),
        }
    }
}

#[cfg(test)]
/// Test module for validating the functionality of RPC methods related to explorer transactions.
/// Focuses on ensuring proper error handling for invalid parameters across several use cases,
/// including cases with missing values, unsupported types, and unparsable inputs.
mod tests {

    use crate::test_utils::{
        setup, validate_invalid_rpc_header_hash, validate_invalid_rpc_tx_hash,
    };

    #[test]
    /// Tests the handling of invalid parameters for the `transactions.get_transactions_by_header_hash` JSON-RPC method.
    /// Verifies that missing and an invalid `header_hash` value results in an appropriate error.
    fn test_transactions_get_transactions_by_header_hash() {
        smol::block_on(async {
            // Define the RPC method name
            let rpc_method = "transactions.get_transactions_by_header_hash";

            // Set up the explorerd
            let explorerd = setup();

            // Validate when provided with an invalid header hash
            validate_invalid_rpc_header_hash(&explorerd, rpc_method);
        });
    }
    #[test]
    /// Tests the handling of invalid parameters for the `transactions.get_transaction_by_hash` JSON-RPC method.
    /// Verifies that missing and an invalid `tx_hash` value results in an appropriate error.
    fn test_transactions_get_transaction_by_hash() {
        smol::block_on(async {
            // Define the RPC method name
            let rpc_method = "transactions.get_transaction_by_hash";

            // Set up the explorerd
            let explorerd = setup();

            // Validate when provided with an invalid tx hash
            validate_invalid_rpc_tx_hash(&explorerd, rpc_method);
        });
    }
}
