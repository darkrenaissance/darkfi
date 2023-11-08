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
use log::{debug, error, info};
use monero::blockdata::transaction::{ExtraField, RawExtraField, SubField::MergeMining};

use super::MiningProxy;

impl MiningProxy {
    /// Perform a oneshot HTTP JSON-RPC request to the set monerod endpoint.
    /// This is a single request-reply which we disconnect after recieving the reply.
    pub async fn oneshot_request(&self, req: JsonRequest) -> Result<JsonValue> {
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
                error!(target: "rpc::monero::oneshot_request", "[RPC] Error sending RPC request to monerod: {}", e);
                return Err(Error::ParseFailed("Failed sending monerod RPC request"))
            }
        };

        let response_bytes = match response.body_bytes().await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rpc::monero::get_block_count", "[RPC] Error reading monerod RPC response: {}", e);
                return Err(Error::ParseFailed("Failed reading monerod RPC reponse"))
            }
        };

        let response_string = match String::from_utf8(response_bytes) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rpc::monero::get_block_count", "[RPC] Error parsing monerod RPC response: {}", e);
                return Err(Error::ParseFailed("Failed parsing monerod RPC reponse"))
            }
        };

        let response_json: JsonValue = match response_string.parse() {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rpc::monero::get_block_count", "[RPC] Error parsing monerod RPC response: {}", e);
                return Err(Error::ParseFailed("Failed parsing monerod RPC reponse"))
            }
        };

        Ok(response_json)
    }

    /// Look up how many blocks are in the longest chain known to the node.
    /// <https://www.getmonero.org/resources/developer-guides/daemon-rpc.html#get_block_count>
    pub async fn monero_get_block_count(&self, id: u16, _params: JsonValue) -> JsonResult {
        debug!(target: "rpc::monero", "get_block_count()");

        // This request can just passthrough
        let req = JsonRequest::new("get_block_count", vec![].into());
        let rep = match self.oneshot_request(req).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rpc::monero::get_block_count", "[RPC] {}", e);
                return JsonError::new(InternalError, Some(e.to_string()), id).into()
            }
        };

        JsonResponse::new(rep, id).into()
    }

    /// Look up a block's hash by its height.
    /// <https://www.getmonero.org/resources/developer-guides/daemon-rpc.html#on_get_block_hash>
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
                error!(target: "rpc::monero::get_block_count", "[RPC] {}", e);
                return JsonError::new(InternalError, Some(e.to_string()), id).into()
            }
        };

        JsonResponse::new(rep, id).into()
    }

    /// Get a block template on which mining a new block.
    /// <https://www.getmonero.org/resources/developer-guides/daemon-rpc.html#get_block_template>
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

        let Some(_reserve_size) = params["reserve_size"].get::<f64>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // The MergeMining tag and anything else going into ExtraField should
        // be done here, so we can pass the correct reserve_size.

        // Create the Merge Mining Tag: (`depth`, `merkle_root`)
        let mm_tag = MergeMining(Some(monero::VarInt(32)), monero::Hash([0_u8; 32]));

        // Construct tx_extra from all the extra fields we have to add to the coinbase
        // transaction in the block we're mining.
        let tx_extra: RawExtraField = ExtraField(vec![mm_tag]).into();

        // Create request. Usually, xmrig will just request a job, so this endpoint
        // isn't really used through JSON-RPC. We use it from other methods, which
        // should then include the proper wallet address to plug in. The wallet
        // address can be set in mmproxy's config or via CLI flags.
        //
        // `reserve_size` is overridden with the size of `tx_extra` created above.
        let req = JsonRequest::new(
            "get_block_template",
            HashMap::from([
                ("wallet_address".to_string(), (*wallet_address).clone().into()),
                ("reserve_size".to_string(), (tx_extra.0.len() as f64).into()),
            ])
            .into(),
        );

        // Get block template from monerod
        let mut rep = match self.oneshot_request(req).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rpc::monero::get_block_template", "[RPC] {}", e);
                return JsonError::new(InternalError, Some(e.to_string()), id).into()
            }
        };

        // Now we have to modify the block template:
        // * reserve_size has to be the size of the data we want to put in the block
        // * blocktemplate_blob has the reserved bytes, they're in the tx_extra field
        //   of the coinbase tx, which is then hashed with other txs into Merkle root
        //   which is what's in the blockhashing_blob

        // Deserialize the block template
        let mut block_template = monero::consensus::deserialize::<monero::Block>(
            &hex::decode(rep["result"]["blocktemplate_blob"].get::<String>().unwrap()).unwrap(),
        )
        .unwrap();

        // Modify the coinbase tx with our additional merge mining data
        block_template.miner_tx.prefix.extra = tx_extra;

        // Replace the blocktemplate blob
        rep["result"]["blocktemplate_blob"] =
            JsonValue::String(hex::encode(monero::consensus::serialize(&block_template)));

        // Replace the blockhashing blob
        rep["result"]["blockhashing_blob"] =
            JsonValue::String(hex::encode(&block_template.serialize_hashable()));

        // Pass the modified response to the client
        JsonResponse::new(rep, id).into()
    }

    /// Submit a mined block to the network
    /// <https://www.getmonero.org/resources/developer-guides/daemon-rpc.html#submit_block>
    pub async fn monero_submit_block(&self, id: u16, params: JsonValue) -> JsonResult {
        debug!(target: "rpc::monero", "submit_block()");

        let Some(params_vec) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        if params_vec.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Deserialize the block blob(s) to make sure it's a valid block
        for element in params_vec.iter() {
            let Some(block_hex) = element.get::<String>() else {
                return JsonError::new(InvalidParams, None, id).into()
            };

            let Ok(block_bytes) = hex::decode(block_hex) else {
                return JsonError::new(InvalidParams, None, id).into()
            };

            let Ok(block) = monero::consensus::deserialize::<monero::Block>(&block_bytes) else {
                return JsonError::new(InvalidParams, None, id).into()
            };

            info!("[RPC] Got submitted Monero block id {}", block.id());
        }

        // Now when all the blocks submitted are valid, we'll just forward them to
        // monerod to submit onto the network.
        let req = JsonRequest::new("submit_block", params);
        let rep = match self.oneshot_request(req).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rpc::monero::submit_block", "[RPC] {}", e);
                return JsonError::new(InternalError, Some(e.to_string()), id).into()
            }
        };

        JsonResponse::new(rep, id).into()
    }

    /*
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
