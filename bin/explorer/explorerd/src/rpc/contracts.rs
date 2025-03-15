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

use std::str::FromStr;

use log::error;
use tinyjson::JsonValue;

use darkfi::rpc::jsonrpc::{
    ErrorCode::{InternalError, InvalidParams},
    JsonError, JsonResponse, JsonResult,
};
use darkfi_sdk::crypto::ContractId;

use crate::Explorerd;

impl Explorerd {
    // RPCAPI:
    // Retrieves the native contracts deployed in the DarkFi network.
    // Returns a JSON array containing Contract IDs along with their associated metadata upon success.
    //
    // **Params:**
    // * `None`
    //
    // **Returns:**
    // * Array of `ContractRecord`s encoded into a JSON.
    //
    // --> {"jsonrpc": "2.0", "method": "contracts.get_native_contracts", "params": ["5cc...2f9"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o", "Money Contract", "The money contract..."], "id": 1}
    pub async fn contracts_get_native_contracts(&self, id: u16, params: JsonValue) -> JsonResult {
        // Ensure that the parameters are empty
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Retrieve native contracts and handle potential errors
        let contract_records = match self.service.get_native_contracts() {
            Ok(v) => v,
            Err(e) => {
                error!(target: "explorerd::rpc_contracts::contracts_get_native_contracts", "Failed fetching native contracts: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        // Transform contract records into a JSON array and return the result
        if contract_records.is_empty() {
            JsonResponse::new(JsonValue::Array(vec![]), id).into()
        } else {
            let json_blocks: Vec<JsonValue> = contract_records
                .into_iter()
                .map(|contract_record| contract_record.to_json_array())
                .collect();
            JsonResponse::new(JsonValue::Array(json_blocks), id).into()
        }
    }

    // RPCAPI:
    // Retrieves the source code paths for the contract associated with the specified Contract ID.
    // Returns a JSON array containing the source code paths upon success.
    //
    // **Params:**
    // * `array[0]`: `String` Contract ID
    //
    // **Returns:**
    // * `JsonArray` containing source code paths for the specified Contract ID.
    //
    // Example Call:
    // --> {"jsonrpc": "2.0", "method": "contracts.get_contract_source_code_paths", "params": ["BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["path/to/source1.rs", "path/to/source2.rs"], "id": 1}
    pub async fn contracts_get_contract_source_code_paths(
        &self,
        id: u16,
        params: JsonValue,
    ) -> JsonResult {
        // Validate that a single required parameter is provided and is of type String
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Validate the provided contract ID and convert it into a ContractId object
        let contact_id_str = params[0].get::<String>().unwrap();
        let contract_id = match ContractId::from_str(contact_id_str) {
            Ok(contract_id) => contract_id,
            Err(e) => return JsonError::new(InternalError, Some(e.to_string()), id).into(),
        };

        // Retrieve source code paths for the contract, transform them into a JsonResponse, and return the result
        match self.service.get_contract_source_paths(&contract_id) {
            Ok(paths) => {
                let transformed_paths =
                    paths.iter().map(|path| JsonValue::String(path.clone())).collect();
                JsonResponse::new(JsonValue::Array(transformed_paths), id).into()
            }
            Err(e) => {
                error!(
                    target: "explorerd::rpc_contracts::contracts_get_contract_source_code_paths",
                    "Failed fetching contract source code paths: {e:?}");
                JsonError::new(InternalError, None, id).into()
            }
        }
    }

    // RPCAPI:
    // Retrieves contract source code content using the provided Contract ID and source path.
    // Returns the source code content as a JSON string upon success.
    //
    // **Params:**
    // * `array[0]`: `String` Contract ID
    // * `array[1]`: `String` Source path
    //
    // **Returns:**
    // * `String` containing the content of the contract source file.
    //
    // Example Call:
    // --> {"jsonrpc": "2.0", "method": "contracts.get_contract_source", "params": ["BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o", "client/lib.rs"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "/* This file is ...", "id": 1}
    pub async fn contracts_get_contract_source(&self, id: u16, params: JsonValue) -> JsonResult {
        // Validate that the required parameters are provided
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 2 || !params[0].is_string() || !params[1].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Validate and extract the provided Contract ID
        let contact_id_str = params[0].get::<String>().unwrap();
        let contract_id = match ContractId::from_str(contact_id_str) {
            Ok(contract_id) => contract_id,
            Err(e) => return JsonError::new(InternalError, Some(e.to_string()), id).into(),
        };

        // Extract the provided source path
        let source_path = params[1].get::<String>().unwrap();

        // Retrieve the contract source code, transform it into a JsonResponse, and return the result
        match self.service.get_contract_source_content(&contract_id, source_path) {
            Ok(Some(source_file)) => JsonResponse::new(JsonValue::String(source_file), id).into(),
            Ok(None) => {
                let empty_value =
                    JsonValue::from(std::collections::HashMap::<String, JsonValue>::new());
                JsonResponse::new(empty_value, id).into()
            }
            Err(e) => {
                error!(
                    target: "explorerd::rpc_contracts::contracts_get_contract_source",
                    "Failed fetching contract source code: {}", e
                );
                JsonError::new(InternalError, None, id).into()
            }
        }
    }
}
