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

use std::{collections::HashSet, time::Instant};

use async_trait::async_trait;
use log::{debug, error, trace};
use smol::lock::MutexGuard;
use tinyjson::JsonValue;

use darkfi::{
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::RequestHandler,
    },
    system::StoppableTaskPtr,
    Result,
};

use crate::{
    error::{server_error, RpcError},
    Explorerd,
};

/// RPC block related requests
pub mod blocks;

/// RPC handlers for contract-related perations
pub mod contracts;

/// RPC handlers for blockchain statistics and metrics
pub mod statistics;

/// RPC handlers for transaction data, lookups, and processing
pub mod transactions;

#[async_trait]
impl RequestHandler<()> for Explorerd {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "explorerd::rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            // =====================
            // Miscellaneous methods
            // =====================
            "ping" => self.pong(req.id, req.params).await,
            "ping_darkfid" => self.ping_darkfid(req.id, req.params).await,

            // =====================
            // Blocks methods
            // =====================
            "blocks.get_last_n_blocks" => self.blocks_get_last_n_blocks(req.id, req.params).await,
            "blocks.get_blocks_in_heights_range" => {
                self.blocks_get_blocks_in_heights_range(req.id, req.params).await
            }
            "blocks.get_block_by_hash" => self.blocks_get_block_by_hash(req.id, req.params).await,

            // =====================
            // Contract methods
            // =====================
            "contracts.get_native_contracts" => {
                self.contracts_get_native_contracts(req.id, req.params).await
            }
            "contracts.get_contract_source_code_paths" => {
                self.contracts_get_contract_source_code_paths(req.id, req.params).await
            }
            "contracts.get_contract_source" => {
                self.contracts_get_contract_source(req.id, req.params).await
            }

            // =====================
            // Transactions methods
            // =====================
            "transactions.get_transactions_by_header_hash" => {
                self.transactions_get_transactions_by_header_hash(req.id, req.params).await
            }
            "transactions.get_transaction_by_hash" => {
                self.transactions_get_transaction_by_hash(req.id, req.params).await
            }

            // =====================
            // Statistics methods
            // =====================
            "statistics.get_basic_statistics" => {
                self.statistics_get_basic_statistics(req.id, req.params).await
            }
            "statistics.get_metric_statistics" => {
                self.statistics_get_metric_statistics(req.id, req.params).await
            }

            // TODO: add any other useful methods

            // ==============
            // Invalid method
            // ==============
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'_, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

impl Explorerd {
    // RPCAPI:
    // Pings configured darkfid daemon for liveness.
    // Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "ping_darkfid", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "true", "id": 1}
    async fn ping_darkfid(&self, id: u16, _params: JsonValue) -> JsonResult {
        debug!(target: "explorerd::rpc::ping_darkfid", "Pinging darkfid daemon...");
        if let Err(e) = self.darkfid_daemon_request("ping", &JsonValue::Array(vec![])).await {
            error!(target: "explorerd::rpc::ping_darkfid", "Failed to ping darkfid daemon: {}", e);
            return server_error(RpcError::PingFailed, id, None)
        }
        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    /// Auxiliary function to execute a request towards the configured darkfid daemon JSON-RPC endpoint.
    pub async fn darkfid_daemon_request(
        &self,
        method: &str,
        params: &JsonValue,
    ) -> Result<JsonValue> {
        debug!(target: "explorerd::rpc::darkfid_daemon_request", "Executing request {} with params: {:?}", method, params);
        let latency = Instant::now();
        let req = JsonRequest::new(method, params.clone());
        let rep = self.rpc_client.request(req).await?;
        let latency = latency.elapsed();
        trace!(target: "explorerd::rpc::darkfid_daemon_request", "Got reply: {:?}", rep);
        debug!(target: "explorerd::rpc::darkfid_daemon_request", "Latency: {:?}", latency);
        Ok(rep)
    }
}
