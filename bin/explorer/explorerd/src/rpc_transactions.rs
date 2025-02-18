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

use log::error;
use tinyjson::JsonValue;

use darkfi::rpc::jsonrpc::{
    ErrorCode::{InternalError, InvalidParams},
    JsonError, JsonResponse, JsonResult,
};
use darkfi_sdk::tx::TransactionHash;

use crate::Explorerd;

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
    // --> {"jsonrpc": "2.0", "method": "transactions.get_transactions_by_header_hash", "params": ["5cc...2f9"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn transactions_get_transactions_by_header_hash(
        &self,
        id: u16,
        params: JsonValue,
    ) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let header_hash = params[0].get::<String>().unwrap();
        let transactions = match self.service.get_transactions_by_header_hash(header_hash) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "explorerd::rpc_transactions::transactions_get_transaction_by_header_hash", "Failed fetching block transactions: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        let mut ret = vec![];
        for transaction in transactions {
            ret.push(transaction.to_json_array());
        }
        JsonResponse::new(JsonValue::Array(ret), id).into()
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
    // --> {"jsonrpc": "2.0", "method": "transactions.get_transaction_by_hash", "params": ["7e7...b4d"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn transactions_get_transaction_by_hash(
        &self,
        id: u16,
        params: JsonValue,
    ) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Validate provided hash and store it for later use
        let tx_hash_str = params[0].get::<String>().unwrap();
        let tx_hash = match tx_hash_str.parse::<TransactionHash>() {
            Ok(hash) => hash,
            Err(e) => return JsonError::new(InternalError, Some(e.to_string()), id).into(),
        };

        // Retrieve transaction by hash and return result
        match self.service.get_transaction_by_hash(&tx_hash) {
            Ok(Some(transaction)) => JsonResponse::new(transaction.to_json_array(), id).into(),
            Ok(None) => JsonResponse::new(JsonValue::Array(vec![]), id).into(),
            Err(e) => {
                error!(target: "explorerd::rpc_transactions::transactions_get_transaction_by_hash", "Failed fetching transaction: {}", e);
                JsonError::new(InternalError, None, id).into()
            }
        }
    }
}
