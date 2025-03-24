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

use std::{collections::HashSet, sync::Arc, time::Instant};

use async_trait::async_trait;
use log::{debug, error, trace, warn};
use smol::lock::{MutexGuard, RwLock};
use tinyjson::JsonValue;
use url::Url;

use darkfi::{
    rpc::{
        client::RpcClient,
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::RequestHandler,
    },
    system::StoppableTaskPtr,
    Error, Result,
};

use crate::{
    error::{server_error, ExplorerdError},
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

/// A RPC client for interacting with a Darkfid JSON-RPC endpoint, enabling communication with Darkfid blockchain nodes.
/// Supports connection management, request handling, and graceful shutdowns.
/// Implemented for shared access across ownership boundaries using `Arc`, with connection state managed via an `RwLock`.
pub struct DarkfidRpcClient {
    /// JSON-RPC client used to communicate with the Darkfid daemon. A value of `None` indicates no active connection.
    /// The `RwLock` allows the client to be shared across ownership boundaries while managing the connection state.
    rpc_client: RwLock<Option<RpcClient>>,
}

impl DarkfidRpcClient {
    /// Creates a new client with an inactive connection.
    pub fn new() -> Self {
        Self { rpc_client: RwLock::new(None) }
    }

    /// Checks if there is an active connection to Darkfid.
    pub async fn connected(&self) -> Result<bool> {
        Ok(self.rpc_client.read().await.is_some())
    }

    /// Establishes a connection to the Darkfid node, storing the resulting client if successful.
    /// If already connected, logs a message and returns without connecting again.
    pub async fn connect(&self, endpoint: Url, ex: Arc<smol::Executor<'static>>) -> Result<()> {
        let mut rpc_client_guard = self.rpc_client.write().await;

        if rpc_client_guard.is_some() {
            warn!(target: "explorerd::rpc::connect", "Already connected to darkfid.");
            return Ok(());
        }

        *rpc_client_guard = Some(RpcClient::new(endpoint, ex).await?);
        Ok(())
    }

    /// Closes the connection with the connected darkfid, returning if there is no active connection.
    /// If the connection is stopped, sets `rpc_client` to `None`.
    pub async fn stop(&self) -> Result<()> {
        let mut rpc_client_guard = self.rpc_client.write().await;

        // If there's an active connection, stop it and clear the reference
        if let Some(ref rpc_client) = *rpc_client_guard {
            rpc_client.stop().await;
            *rpc_client_guard = None;
            return Ok(());
        }

        // If there's no connection, log the message and do nothing
        warn!(target: "explorerd::rpc::stop", "Not connected to darkfid, nothing to stop.");
        Ok(())
    }

    /// Sends a request to the client's Darkfid JSON-RPC endpoint using the given method and parameters.
    /// Returns the received response or an error if no active connection to Darkfid exists.
    pub async fn request(&self, method: &str, params: &JsonValue) -> Result<JsonValue> {
        let rpc_client_guard = self.rpc_client.read().await;

        if let Some(ref rpc_client) = *rpc_client_guard {
            debug!(target: "explorerd::rpc::request", "Executing request {} with params: {:?}", method, params);
            let latency = Instant::now();
            let req = JsonRequest::new(method, params.clone());
            let rep = rpc_client.request(req).await?;
            let latency = latency.elapsed();
            trace!(target: "explorerd::rpc::request", "Got reply: {:?}", rep);
            debug!(target: "explorerd::rpc::request", "Latency: {:?}", latency);
            return Ok(rep);
        };

        error!(target: "explorerd::rpc::request", "Not connected to darkfid.");
        Err(Error::Custom(
            "Not connected to darkfid. Is the explorer running in no-sync mode?".to_string(),
        ))
    }

    /// Sends a ping request to the client's darkfid endpoint to verify connectivity,
    /// returning `true` if the ping is successful or an error if the request fails.
    async fn ping(&self) -> Result<bool> {
        if let Err(e) = self.request("ping", &JsonValue::Array(vec![])).await {
            error!(target: "explorerd::rpc::ping", "Failed to ping darkfid daemon: {}", e);
            return Err(e);
        }

        Ok(true)
    }
}

impl Default for DarkfidRpcClient {
    fn default() -> Self {
        Self::new()
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
        if let Err(e) = self.darkfid_client.ping().await {
            error!(target: "explorerd::rpc::ping_darkfid", "Failed to ping darkfid daemon: {}", e);
            return server_error(&ExplorerdError::PingDarkfidFailed(e.to_string()), id, None).into()
        }
        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }
}
