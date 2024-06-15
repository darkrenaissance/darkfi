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

use log::error;
use tinyjson::JsonValue;

use darkfi::rpc::jsonrpc::{
    ErrorCode::{InternalError, InvalidParams},
    JsonError, JsonResponse, JsonResult,
};

use crate::BlockchainExplorer;

impl BlockchainExplorer {
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
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let base_statistics = match self.get_base_statistics().await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "blockchain-explorer::rpc_statistics::statistics_get_basic_statistics", "Failed fetching basic statistics: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        JsonResponse::new(base_statistics.to_json_array(), id).into()
    }
}
