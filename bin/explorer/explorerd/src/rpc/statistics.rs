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

use std::vec::Vec;

use tinyjson::JsonValue;

use darkfi::{rpc::jsonrpc::validate_empty_params, Result};

use crate::Explorerd;

impl Explorerd {
    // RPCAPI:
    // Queries the database to retrieve current basic statistics.
    // Returns the readable transaction upon success.
    //
    // **Params:**
    // * `None`
    //
    // **Returns:**
    // * `BaseStatistics` encoded into a JSON.
    //
    // **Example API Usage:**
    // --> {"jsonrpc": "2.0", "method": "statistics.get_basic_statistics", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn statistics_get_basic_statistics(&self, params: &JsonValue) -> Result<JsonValue> {
        // Validate that no parameters are provided
        validate_empty_params(params)?;

        // Attempt to retrieve base statistics; if found, convert to a JSON array,
        // otherwise return an empty JSON array
        match self.service.get_base_statistics()? {
            Some(statistics) => Ok(statistics.to_json_array()),
            None => Ok(JsonValue::Array(vec![])),
        }
    }

    // RPCAPI:
    // Queries the database to retrieve all metrics statistics.
    // Returns a collection of metric statistics upon success.
    //
    // **Params:**
    // * `None`
    //
    // **Returns:**
    // * `MetricsStatistics` array encoded into a JSON.
    //
    // **Example API Usage:**
    // --> {"jsonrpc": "2.0", "method": "statistics.get_metric_statistics", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn statistics_get_metric_statistics(&self, params: &JsonValue) -> Result<JsonValue> {
        // Validate that no parameters are provided
        validate_empty_params(params)?;

        // Retrieve metric statistics
        let statistics = self.service.get_metrics_statistics().await?;

        // Convert each metric statistic into a JSON array, returning the collected array
        let statistics_json: Vec<JsonValue> =
            statistics.iter().map(|m| m.to_json_array()).collect();
        Ok(JsonValue::Array(statistics_json))
    }

    // RPCAPI:
    // Queries the database to retrieve latest metric statistics.
    // Returns the readable metric statistics upon success.
    //
    // **Params:**
    // * `None`
    //
    // **Returns:**
    // * `MetricsStatistics` encoded into a JSON.
    //
    // **Example API Usage:**
    // --> {"jsonrpc": "2.0", "method": "statistics.get_latest_metric_statistics", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn statistics_get_latest_metric_statistics(
        &self,
        params: &JsonValue,
    ) -> Result<JsonValue> {
        // Validate that no parameters are provided
        validate_empty_params(params)?;

        // Retrieve the latest statistics
        let statistics = self.service.get_latest_metrics_statistics().await?;

        // Convert the retrieved metrics into a JSON array and return it
        Ok(statistics.to_json_array())
    }
}

#[cfg(test)]
/// Test module for validating the functionality of RPC methods related to explorer statistics.
/// Focuses on ensuring proper error handling for invalid parameters across several use cases.
mod tests {

    use crate::test_utils::{setup, validate_empty_rpc_parameters};

    /// Tests all RPC-related statistics calls when provided with empty parameters, ensuring they
    /// handle the input correctly and return appropriate validation responses.
    #[test]
    fn test_statistics_rpc_calls_for_empty_parameters() {
        smol::block_on(async {
            let explorerd = setup();

            let rpc_methods = [
                "statistics.get_latest_metric_statistics",
                "statistics.get_metric_statistics",
                "statistics.get_basic_statistics",
            ];

            for rpc_method in rpc_methods.iter() {
                validate_empty_rpc_parameters(&explorerd, rpc_method).await;
            }
        });
    }
}
