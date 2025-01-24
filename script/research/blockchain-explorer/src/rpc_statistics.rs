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

use log::error;
use tinyjson::JsonValue;

use darkfi::rpc::jsonrpc::{
    ErrorCode::{InternalError, InvalidParams},
    JsonError, JsonResponse, JsonResult,
};

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
    // --> {"jsonrpc": "2.0", "method": "statistics.get_basic_statistics", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn statistics_get_basic_statistics(&self, id: u16, params: JsonValue) -> JsonResult {
        // Validate to ensure parameters are empty
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Fetch `BaseStatistics`, transform to `JsonResult`, and return results
        match self.service.get_base_statistics() {
            Ok(Some(statistics)) => JsonResponse::new(statistics.to_json_array(), id).into(),
            Ok(None) => JsonResponse::new(JsonValue::Array(vec![]), id).into(),
            Err(e) => {
                error!(
                    target: "blockchain-explorer::rpc_statistics::statistics_get_basic_statistics",
                    "Failed fetching basic statistics: {}", e
                );
                JsonError::new(InternalError, None, id).into()
            }
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
    // --> {"jsonrpc": "2.0", "method": "statistics.get_metric_statistics", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn statistics_get_metric_statistics(&self, id: u16, params: JsonValue) -> JsonResult {
        // Validate to ensure parameters are empty
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        // Fetch metric statistics and return results
        let metrics = match self.service.get_metrics_statistics().await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "blockchain-explorer::rpc_statistics::statistics_get_metric_statistics", "Failed fetching metric statistics: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        // Transform statistics to JsonResponse and return result
        let metrics_json: Vec<JsonValue> = metrics.iter().map(|m| m.to_json_array()).collect();
        JsonResponse::new(JsonValue::Array(metrics_json), id).into()
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
    // --> {"jsonrpc": "2.0", "method": "statistics.get_latest_metric_statistics", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn statistics_get_latest_metric_statistics(
        &self,
        id: u16,
        params: JsonValue,
    ) -> JsonResult {
        // Validate to ensure parameters are empty
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        // Fetch metric statistics and return results
        let metrics = match self.service.get_latest_metrics_statistics().await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "blockchain-explorer::rpc_statistics::statistics_get_latest_metric_statistics", "Failed fetching metric statistics: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        // Transform statistics to JsonResponse and return result
        JsonResponse::new(metrics.to_json_array(), id).into()
    }
}
