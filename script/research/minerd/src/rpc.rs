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

use std::collections::HashSet;

use log::{debug, error, info};
use num_bigint::BigUint;
use smol::lock::MutexGuard;

use darkfi::{
    blockchain::BlockInfo,
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::RequestHandler,
        util::JsonValue,
    },
    system::StoppableTaskPtr,
    util::encoding::base64,
    validator::pow::mine_block,
};
use darkfi_sdk::num_traits::Num;
use darkfi_serial::{async_trait, deserialize, serialize};

use crate::{
    error::{server_error, RpcError},
    Minerd,
};

#[async_trait]
impl RequestHandler for Minerd {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "minerd::rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            "ping" => self.pong(req.id, req.params).await,
            "mine" => self.mine(req.id, req.params).await,
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'_, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

impl Minerd {
    // RPCAPI:
    // Mine provided block for requested mine target, and return the corresponding nonce value.
    //
    // --> {"jsonrpc": "2.0", "method": "mine", "params": ["target", "block"], "id": 42}
    // --> {"jsonrpc": "2.0", "result": "nonce", "id": 42}
    async fn mine(&self, id: u16, params: JsonValue) -> JsonResult {
        // Verify parameters
        if !params.is_array() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 2 || !params[0].is_string() || !params[1].is_string() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        // Parse parameters
        let Ok(target) = BigUint::from_str_radix(params[0].get::<String>().unwrap(), 10) else {
            error!(target: "minerd::rpc", "Failed to parse target");
            return server_error(RpcError::TargetParseError, id, None)
        };
        let Some(block_bytes) = base64::decode(params[1].get::<String>().unwrap()) else {
            error!(target: "minerd::rpc", "Failed to parse block bytes");
            return server_error(RpcError::BlockParseError, id, None)
        };
        let Ok(mut block) = deserialize::<BlockInfo>(&block_bytes) else {
            error!(target: "minerd::rpc", "Failed to parse block");
            return server_error(RpcError::BlockParseError, id, None)
        };

        // Mine provided block
        let Ok(block_hash) = block.hash() else {
            error!(target: "minerd::rpc", "Failed to hash block");
            return server_error(RpcError::HashingFailed, id, None)
        };
        info!(target: "minerd::rpc", "Mining block {} for target: {}", block_hash, target);
        if let Err(e) = mine_block(&target, &mut block, self.threads, &self.stop_signal) {
            error!(target: "minerd::rpc", "Failed mining block {} with error: {}", block_hash, e);
            return server_error(RpcError::MiningFailed, id, None)
        }

        // Return block nonce
        let nonce = base64::encode(&serialize(&block.header.nonce));
        JsonResponse::new(JsonValue::String(nonce), id).into()
    }
}
