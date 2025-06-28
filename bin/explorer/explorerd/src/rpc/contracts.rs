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

use tinyjson::JsonValue;

use darkfi::{
    rpc::jsonrpc::{parse_json_array_string, validate_empty_params},
    Result,
};
use darkfi_sdk::crypto::ContractId;

use crate::{error::ExplorerdError, Explorerd};

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
    // **Example API Usage:**
    // --> {"jsonrpc": "2.0", "method": "contracts.get_native_contracts", "params": ["5cc...2f9"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o", "Money Contract", "The money contract..."], "id": 1}
    pub async fn contracts_get_native_contracts(&self, params: &JsonValue) -> Result<JsonValue> {
        // Validate that no parameters are provided
        validate_empty_params(params)?;

        // Retrieve native contracts
        let contract_records = self.service.get_native_contracts()?;

        // Transform contract records into a JSON array and return result
        if contract_records.is_empty() {
            Ok(JsonValue::Array(vec![]))
        } else {
            let json_blocks: Vec<JsonValue> = contract_records
                .into_iter()
                .map(|contract_record| contract_record.to_json_array())
                .collect();
            Ok(JsonValue::Array(json_blocks))
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
    // **Example API Usage:**
    // --> {"jsonrpc": "2.0", "method": "contracts.get_contract_source_code_paths", "params": ["BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["path/to/source1.rs", "path/to/source2.rs"], "id": 1}
    pub async fn contracts_get_contract_source_code_paths(
        &self,
        params: &JsonValue,
    ) -> Result<JsonValue> {
        // Extract contract ID
        let contact_id_str = parse_json_array_string("contract_id", 0, params)?;

        // Convert the contract string to a `ContractId` instance
        let contract_id = ContractId::from_str(&contact_id_str)
            .map_err(|_| ExplorerdError::InvalidContractId(contact_id_str))?;

        // Retrieve source code paths for the contract
        let paths = self.service.get_contract_source_paths(&contract_id)?;

        // Tranform found paths into `JsonValues`
        let json_value_paths = paths.iter().map(|path| JsonValue::String(path.clone())).collect();

        Ok(JsonValue::Array(json_value_paths))
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
    // **Example API Usage:**
    // --> {"jsonrpc": "2.0", "method": "contracts.get_contract_source", "params": ["BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o", "client/lib.rs"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "/* This file is ...", "id": 1}
    pub async fn contracts_get_contract_source(&self, params: &JsonValue) -> Result<JsonValue> {
        // Extract the contract ID
        let contact_id_str = parse_json_array_string("contract_id", 0, params)?;

        // Convert the contract string to a `ContractId` instance
        let contract_id = ContractId::from_str(&contact_id_str)
            .map_err(|_| ExplorerdError::InvalidContractId(contact_id_str))?;

        // Extract the source path
        let source_path = parse_json_array_string("source_path", 1, params)?;

        // Retrieve the contract source code, transform it into a `JsonValue`, and return the result
        match self.service.get_contract_source_content(&contract_id, &source_path)? {
            Some(source_file) => Ok(JsonValue::String(source_file)),
            None => Ok(JsonValue::from(std::collections::HashMap::<String, JsonValue>::new())),
        }
    }
}

#[cfg(test)]
/// Test module for validating the functionality of RPC methods related to explorer contracts.
/// Focuses on ensuring proper error handling for invalid parameters across several use cases,
/// including cases with missing values, unsupported types, and unparsable inputs.
mod tests {

    use tinyjson::JsonValue;

    use darkfi::rpc::jsonrpc::ErrorCode;

    use crate::test_utils::{
        setup, validate_empty_rpc_parameters, validate_invalid_rpc_contract_id,
        validate_invalid_rpc_parameter,
    };

    #[test]
    /// Tests the `contracts.get_native_contracts` method to ensure it correctly handles cases where
    /// empty parameters are supplied, returning an expected result or error response.
    fn test_contracts_get_native_contracts_empty_params() {
        smol::block_on(async {
            validate_empty_rpc_parameters(&setup(), "contracts.get_native_contracts").await;
        });
    }

    #[test]
    /// Tests the `contracts.get_contract_source_code_paths` method to ensure it correctly handles cases
    /// with invalid or missing `contract_id` parameters, returning appropriate error responses.
    fn test_contracts_get_contract_source_code_paths_invalid_params() {
        validate_invalid_rpc_contract_id(&setup(), "contracts.get_contract_source_code_paths");
    }

    #[test]
    /// Tests the `contracts.get_contract_source` method to ensure it correctly handles cases
    /// with invalid or missing parameters, returning appropriate error responses.
    fn test_contracts_get_contract_source_invalid_params() {
        let test_method = "contracts.get_contract_source";
        let parameter_name = "source_path";

        smol::block_on(async {
            // Set up the explorerd instance
            let explorerd = setup();

            validate_invalid_rpc_contract_id(&explorerd, test_method);

            // Test for missing `source_path` parameter
            validate_invalid_rpc_parameter(
                &explorerd,
                test_method,
                &[JsonValue::String("BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o".to_string())],
                ErrorCode::InvalidParams.code(),
                &format!("Parameter '{parameter_name}' at index 1 is missing"),
            )
            .await;

            // Test for invalid `source_path` parameter
            validate_invalid_rpc_parameter(
                &explorerd,
                test_method,
                &[
                    JsonValue::String("BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o".to_string()),
                    JsonValue::Number(123.0), // Invalid `source_path` type
                ],
                ErrorCode::InvalidParams.code(),
                &format!("Parameter '{parameter_name}' is not a valid string"),
            )
            .await;
        });
    }
}
