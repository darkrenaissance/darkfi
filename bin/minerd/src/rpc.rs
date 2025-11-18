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

use std::collections::HashSet;

use num_bigint::BigUint;
use smol::lock::MutexGuard;
use tracing::{debug, error, info};

use darkfi::{
    blockchain::header_store::{Header, HeaderHash},
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::RequestHandler,
        util::JsonValue,
    },
    system::{sleep, StoppableTaskPtr},
    util::encoding::base64,
    validator::pow::mine_block,
};
use darkfi_serial::{async_trait, deserialize_async};

use crate::{
    error::{server_error, RpcError},
    MinerNode,
};

#[async_trait]
impl RequestHandler<()> for MinerNode {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "minerd::rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            "ping" => self.pong(req.id, req.params).await,
            "abort" => self.abort(req.id, req.params).await,
            "mine" => self.mine(req.id, req.params).await,
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

impl MinerNode {
    // RPCAPI:
    // Signals miner daemon to abort mining pending request.
    // Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "abort", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "true", "id": 42}
    async fn abort(&self, id: u16, _params: JsonValue) -> JsonResult {
        if let Some(e) = self.abort_pending(id).await {
            return e
        };
        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Mine provided block header for requested mine target, using
    // provided RandomX VM key, and return the corresponding nonce
    // value.
    //
    // --> {"jsonrpc": "2.0", "method": "mine", "params": ["target", "randomx_key", "header"], "id": 42}
    // --> {"jsonrpc": "2.0", "result": "nonce", "id": 42}
    async fn mine(&self, id: u16, params: JsonValue) -> JsonResult {
        // Verify parameters
        if !params.is_array() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 3 ||
            !params[0].is_string() ||
            !params[1].is_string() ||
            !params[2].is_string()
        {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        // Parse parameters
        let Some(target_bytes) = base64::decode(params[0].get::<String>().unwrap()) else {
            error!(target: "minerd::rpc", "Failed to parse target bytes");
            return server_error(RpcError::TargetParseError, id, None)
        };
        let target = BigUint::from_bytes_le(&target_bytes);
        let Some(randomx_key_bytes) = base64::decode(params[1].get::<String>().unwrap()) else {
            error!(target: "minerd::rpc", "Failed to parse RandomX key bytes");
            return server_error(RpcError::BlockParseError, id, None)
        };
        let Ok(randomx_key) = deserialize_async::<[u8; 32]>(&randomx_key_bytes).await else {
            error!(target: "minerd::rpc", "Failed to parse RandomX key");
            return server_error(RpcError::BlockParseError, id, None)
        };
        let Some(header_bytes) = base64::decode(params[2].get::<String>().unwrap()) else {
            error!(target: "minerd::rpc", "Failed to parse header bytes");
            return server_error(RpcError::BlockParseError, id, None)
        };
        let Ok(mut header) = deserialize_async::<Header>(&header_bytes).await else {
            error!(target: "minerd::rpc", "Failed to parse header");
            return server_error(RpcError::BlockParseError, id, None)
        };
        let header_hash = header.hash();
        let randomx_key_hash = HeaderHash::new(randomx_key);
        info!(target: "minerd::rpc", "Received request to mine block header {header_hash} with key {randomx_key_hash} for target: {target}");

        // If we have a requested mining height, we'll keep dropping here.
        if self.stop_at_height > 0 && header.height >= self.stop_at_height {
            info!(target: "minerd::rpc", "Reached requested mining height {}", self.stop_at_height);
            return server_error(RpcError::MiningFailed, id, None)
        }

        // Check if another request is being processed
        if let Some(e) = self.abort_pending(id).await {
            return e
        };

        // Mine provided block header
        info!(target: "minerd::rpc", "Mining block header {header_hash} with key {randomx_key_hash} for target: {target}");
        if let Err(e) =
            mine_block(&target, &randomx_key, &mut header, self.threads, &self.stop_signal.clone())
        {
            error!(target: "minerd::rpc", "Failed mining block header {header_hash} with error: {e}");
            return server_error(RpcError::MiningFailed, id, None)
        }
        info!(target: "minerd::rpc", "Mined block header {header_hash} with nonce: {}", header.nonce);

        // Return block header nonce
        JsonResponse::new(JsonValue::Number(header.nonce as f64), id).into()
    }

    /// Auxiliary function to abort pending request.
    async fn abort_pending(&self, id: u16) -> Option<JsonResult> {
        // Check if a pending request is being processed
        info!(target: "minerd::rpc", "Checking if a pending request is being processed...");
        if self.stop_signal.receiver_count() <= 1 {
            info!(target: "minerd::rpc", "No pending requests!");
            return None
        }

        info!(target: "minerd::rpc", "Pending request is in progress, sending stop signal...");
        // Send stop signal to worker
        if self.sender.send(()).await.is_err() {
            error!(target: "minerd::rpc", "Failed to stop pending request");
            return Some(server_error(RpcError::StopFailed, id, None))
        }

        // Wait for worker to terminate
        info!(target: "minerd::rpc", "Waiting for request to terminate...");
        while self.stop_signal.receiver_count() > 1 {
            sleep(1).await;
        }
        info!(target: "minerd::rpc", "Pending request terminated!");

        // Consume channel item so its empty again
        if self.stop_signal.recv().await.is_err() {
            error!(target: "minerd::rpc", "Failed to cleanup stop signal channel");
            return Some(server_error(RpcError::StopFailed, id, None))
        }

        None
    }
}
