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

use tinyjson::JsonValue;

use darkfi::{
    blockchain::BlockInfo,
    error::RpcError,
    rpc::jsonrpc::{parse_json_array_number, parse_json_array_string},
    util::encoding::base64,
    Result,
};
use darkfi_serial::deserialize_async;

use crate::{rpc::DarkfidRpcClient, Explorerd};

impl DarkfidRpcClient {
    /// Retrieves a block from at a given height returning the corresponding [`BlockInfo`].
    pub async fn get_block_by_height(&self, height: u32) -> Result<BlockInfo> {
        let params = self
            .request(
                "blockchain.get_block",
                &JsonValue::Array(vec![JsonValue::String(height.to_string())]),
            )
            .await?;
        let param = params.get::<String>().unwrap();
        let bytes = base64::decode(param).unwrap();
        let block = deserialize_async(&bytes).await?;
        Ok(block)
    }

    /// Retrieves the last confirmed block returning the block height and its header hash.
    pub async fn get_last_confirmed_block(&self) -> Result<(u32, String)> {
        let rep =
            self.request("blockchain.last_confirmed_block", &JsonValue::Array(vec![])).await?;
        let params = rep.get::<Vec<JsonValue>>().unwrap();
        let height = *params[0].get::<f64>().unwrap() as u32;
        let hash = params[1].get::<String>().unwrap().clone();

        Ok((height, hash))
    }
}

impl Explorerd {
    // RPCAPI:
    // Queries the database to retrieve last N blocks.
    // Returns an array of readable blocks upon success.
    //
    // **Params:**
    // * `array[0]`: `u16` Number of blocks to retrieve (as string)
    //
    // **Returns:**
    // * Array of `BlockRecord` encoded into a JSON.
    //
    // **Example API Usage:**
    // --> {"jsonrpc": "2.0", "method": "blocks.get_last_n_blocks", "params": [10], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn blocks_get_last_n_blocks(&self, params: &JsonValue) -> Result<JsonValue> {
        // Extract the number of last blocks to fetch
        let num_last_blocks = parse_json_array_number("num_last_blocks", 0, params)? as usize;

        // Fetch the blocks
        let blocks_result = self.service.get_last_n(num_last_blocks)?;

        // Transform blocks to `JsonValue`
        if blocks_result.is_empty() {
            Ok(JsonValue::Array(vec![]))
        } else {
            let json_blocks: Vec<JsonValue> =
                blocks_result.into_iter().map(|block| block.to_json_array()).collect();
            Ok(JsonValue::Array(json_blocks))
        }
    }

    // RPCAPI:
    // Queries the database to retrieve blocks in provided heights range.
    // Returns an array of readable blocks upon success.
    //
    // **Params:**
    // * `array[0]`: `u32` Starting height (as string)
    // * `array[1]`: `u32` Ending height range (as string)
    //
    // **Returns:**
    // * Array of `BlockRecord` encoded into a JSON.
    //
    // **Example API Usage:**
    // --> {"jsonrpc": "2.0", "method": "blocks.get_blocks_in_heights_range", "params": [10, 15], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn blocks_get_blocks_in_heights_range(
        &self,
        params: &JsonValue,
    ) -> Result<JsonValue> {
        // Extract the start range
        let start = parse_json_array_number("start", 0, params)? as u32;

        // Extract the end range
        let end = parse_json_array_number("end", 1, params)? as u32;

        // Validate for valid range
        if start > end {
            return Err(RpcError::InvalidJson(format!(
                "Invalid range: start ({start}) cannot be greater than end ({end})"
            ))
            .into());
        }

        // Fetch the blocks
        let blocks_result = self.service.get_by_range(start, end)?;

        // Transform blocks to `JsonValue` and return result
        if blocks_result.is_empty() {
            Ok(JsonValue::Array(vec![]))
        } else {
            let json_blocks: Vec<JsonValue> =
                blocks_result.into_iter().map(|block| block.to_json_array()).collect();
            Ok(JsonValue::Array(json_blocks))
        }
    }

    // RPCAPI:
    // Queries the database to retrieve the block corresponding to the provided hash.
    // Returns the readable block upon success.
    //
    // **Params:**
    // * `array[0]`: `String` Block header hash
    //
    // **Returns:**
    // * `BlockRecord` encoded into a JSON.
    //
    // **Example API Usage:**
    // --> {"jsonrpc": "2.0", "method": "blocks.get_block_by_hash", "params": ["5cc...2f9"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn blocks_get_block_by_hash(&self, params: &JsonValue) -> Result<JsonValue> {
        // Extract header hash
        let header_hash = parse_json_array_string("header_hash", 0, params)?;

        // Fetch and transform block to `JsonValue`
        match self.service.get_block_by_hash(&header_hash)? {
            Some(block) => Ok(block.to_json_array()),
            None => Ok(JsonValue::Array(vec![])),
        }
    }
}
