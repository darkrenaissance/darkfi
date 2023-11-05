/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use std::collections::HashMap;

use darkfi::{
    rpc::{
        jsonrpc::{
            ErrorCode::{InternalError, InvalidParams},
            JsonError, JsonRequest, JsonResponse, JsonResult,
        },
        util::JsonValue,
    },
    Error, Result,
};
use log::{debug, error};

use super::MiningProxy;

impl MiningProxy {
    async fn oneshot_request(&self, req: JsonRequest) -> Result<JsonValue> {
        let client = surf::Client::new();

        let mut response = match client
            .get(&self.monerod.monerod_rpc)
            .header("Content-Type", "application/json")
            .body(req.stringify().unwrap())
            .send()
            .await
        {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rpc::monero::oneshot_request", "Error sending RPC request to monerod: {}", e);
                return Err(Error::ParseFailed("Failed sending monerod RPC request"))
            }
        };

        let response_bytes = match response.body_bytes().await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rpc::monero::get_block_count", "Error reading monerod RPC response: {}", e);
                return Err(Error::ParseFailed("Failed reading monerod RPC reponse"))
            }
        };

        let response_string = match String::from_utf8(response_bytes) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rpc::monero::get_block_count", "Error parsing monerod RPC response: {}", e);
                return Err(Error::ParseFailed("Failed parsing monerod RPC reponse"))
            }
        };

        let response_json: JsonValue = match response_string.parse() {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rpc::monero::get_block_count", "Error parsing monerod RPC response: {}", e);
                return Err(Error::ParseFailed("Failed parsing monerod RPC reponse"))
            }
        };

        Ok(response_json)
    }

    pub async fn monero_get_block_count(&self, id: u16, _params: JsonValue) -> JsonResult {
        debug!(target: "rpc::monero", "get_block_count()");

        let req = JsonRequest::new("get_block_count", vec![].into());
        let rep = match self.oneshot_request(req).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rpc::monero::get_block_count", "{}", e);
                return JsonError::new(InternalError, Some(e.to_string()), id).into()
            }
        };

        JsonResponse::new(rep, id).into()
    }

    pub async fn monero_on_get_block_hash(&self, id: u16, params: JsonValue) -> JsonResult {
        debug!(target: "rpc::monero", "on_get_block_hash()");

        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        if !params.len() != 1 || params[0].is_number() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let Some(block_height) = params[0].get::<f64>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        let req =
            JsonRequest::new("on_get_block_hash", vec![JsonValue::Number(*block_height)].into());
        let rep = match self.oneshot_request(req).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rpc::monero::get_block_count", "{}", e);
                return JsonError::new(InternalError, Some(e.to_string()), id).into()
            }
        };

        JsonResponse::new(rep, id).into()
    }

    pub async fn monero_get_block_template(&self, id: u16, params: JsonValue) -> JsonResult {
        debug!(target: "rpc::monero", "get_block_template()");

        if !params.is_object() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let params = params.get::<HashMap<String, JsonValue>>().unwrap();

        if !params.contains_key("wallet_address") || !params.contains_key("reserve_size") {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let Some(wallet_address) = params["wallet_address"].get::<String>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        let Some(reserve_size) = params["reserve_size"].get::<f64>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Create request
        let req = JsonRequest::new(
            "get_block_template",
            HashMap::from([
                ("wallet_address".to_string(), (*wallet_address).clone().into()),
                ("reserve_size".to_string(), (*reserve_size).into()),
            ])
            .into(),
        );

        let rep = match self.oneshot_request(req).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rpc::monero::get_block_template", "{}", e);
                return JsonError::new(InternalError, Some(e.to_string()), id).into()
            }
        };

        // TODO: Now we have to modify the block template.
        // * reserve_size has to be the size of the data we want to put in the block
        // * blocktemplate_blob has the reserved bytes, they're in the tx_extra field
        //   of the coinbase tx, which is then hashed with other transactions into
        //   merkle root hash which is what's in the blockhashing_blob
        // * When blocktemplate_blob is modified, blockhashing_blob has to be updated too
        // * The coinbase tx from monerod should be replaced with the one we create
        //   containing the merged mining data/info

        JsonResponse::new(rep, id).into()
    }

    /*
    pub async fn monero_submit_block(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_generateblocks(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_last_block_header(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_block_header_by_hash(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_block_header_by_height(
        &self,
        id: u16,
        params: JsonValue,
    ) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_block_headers_range(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_block(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_connections(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_info(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_hard_fork_info(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_set_bans(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_bans(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_banned(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_flush_txpool(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_output_histogram(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_version(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_coinbase_tx_sum(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_fee_estimate(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_alternate_chains(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_relay_tx(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_sync_info(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_txpool_backlog(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_output_distribution(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_get_miner_data(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_prune_blockchain(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_calc_pow(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_flush_cache(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    pub async fn monero_add_aux_pow(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }
    */
}
