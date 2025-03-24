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
    error::RpcError,
    rpc::{
        client::RpcClient,
        jsonrpc::{
            validate_empty_params, ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult,
        },
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
    /// Handles an incoming JSON-RPC request by executing the appropriate individual request handler
    /// implementation based on the request's `method` field and using the provided parameters.
    /// Supports methods across various categories, including block-related queries, contract interactions,
    /// transaction lookups, statistical queries, and miscellaneous operations. If an invalid
    /// method is requested, an appropriate error is returned.
    ///
    /// The function performs the error handling, allowing individual RPC method handlers to propagate
    /// errors via the `?` operator. It ensures uniform translation of errors into JSON-RPC error responses.
    /// Additionally, it handles the creation of `JsonResponse` or `JsonError` objects, enabling method
    /// handlers to focus solely on core logic. Individual RPC handlers return a `JsonValue`, which this
    /// function translates into the corresponding `JsonResult`.
    ///
    /// Unified logging is incorporated, so individual handlers only propagate the error
    /// for it to be logged. Logs include detailed error information, such as method names, parameters,
    /// and JSON-RPC errors, providing consistent and informative error trails for debugging.
    ///
    /// ## Example Log Message
    /// ```
    /// 05:11:02 [ERROR] RPC Request Failure: method: transactions.get_transactions_by_header_hash,
    /// params: ["0x0222"], error: {"error":{"code":-32602,"message":"Invalid header hash: 0x0222"},
    /// "id":1,"jsonrpc":"2.0"}
    /// ```
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "explorerd::rpc", "--> {}", req.stringify().unwrap());

        // Store method and params for later use
        let method = req.method.as_str();
        let params = &req.params;

        // Handle ping case, as it returns a JsonResponse
        if method == "ping" {
            return self.pong(req.id, params.clone()).await
        }

        // Match all other methods
        let result = match req.method.as_str() {
            // =====================
            // Blocks methods
            // =====================
            "blocks.get_last_n_blocks" => self.blocks_get_last_n_blocks(params).await,
            "blocks.get_blocks_in_heights_range" => {
                self.blocks_get_blocks_in_heights_range(params).await
            }
            "blocks.get_block_by_hash" => self.blocks_get_block_by_hash(params).await,

            // =====================
            // Transactions methods
            // =====================
            "transactions.get_transactions_by_header_hash" => {
                self.transactions_get_transactions_by_header_hash(params).await
            }
            "transactions.get_transaction_by_hash" => {
                self.transactions_get_transaction_by_hash(params).await
            }

            // =====================
            // Statistics methods
            // =====================
            "statistics.get_basic_statistics" => self.statistics_get_basic_statistics(params).await,
            "statistics.get_metric_statistics" => {
                self.statistics_get_metric_statistics(params).await
            }
            "statistics.get_latest_metric_statistics" => {
                self.statistics_get_latest_metric_statistics(params).await
            }

            // =====================
            // Contract methods
            // =====================
            "contracts.get_native_contracts" => self.contracts_get_native_contracts(params).await,
            "contracts.get_contract_source_code_paths" => {
                self.contracts_get_contract_source_code_paths(params).await
            }
            "contracts.get_contract_source" => self.contracts_get_contract_source(params).await,

            // =====================
            // Miscellaneous methods
            // =====================
            "ping_darkfid" => self.ping_darkfid(params).await,

            // TODO: add any other useful methods

            // ==============
            // Invalid method
            // ==============
            _ => Err(RpcError::MethodNotFound(method.to_string()).into()),
        };

        // Process the result of the individual request handler, handling success or errors and translating
        // them into an appropriate `JsonResult`.
        match result {
            // Successfully completed the request
            Ok(value) => JsonResponse::new(value, req.id).into(),

            // Handle errors when processing parameters
            Err(Error::RpcServerError(RpcError::InvalidJson(e))) => {
                let json_error =
                    JsonError::new(ErrorCode::InvalidParams, Some(e.to_string()), req.id);

                // Log the parameter error
                log_request_failure(&req.method, params, &json_error);

                // Convert error to JsonResult
                json_error.into()
            }

            // Handle server errors
            Err(Error::RpcServerError(RpcError::ServerError(e))) => {
                // Remove the extra '&' and reference directly from e
                let json_error = match e.downcast_ref::<ExplorerdError>() {
                    Some(e_expl) => {
                        // Successfully downcast to ExplorerdRpcError; call the typed function
                        server_error(e_expl, req.id, None)
                    }
                    None => {
                        // Return InternalError with the logged details
                        JsonError::new(ErrorCode::InternalError, Some(e.to_string()), req.id)
                    }
                };

                // Log the server error
                log_request_failure(&req.method, params, &json_error);

                // Convert error to JsonResult
                json_error.into()
            }

            // Catch-all for any other unexpected errors
            Err(e) => {
                // Return InternalError with the logged details
                let json_error =
                    JsonError::new(ErrorCode::InternalError, Some(e.to_string()), req.id);

                // Log the unexpected error
                log_request_failure(&req.method, params, &json_error);

                // Convert error to JsonResult
                json_error.into()
            }
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
            warn!(target: "explorerd::rpc::connect", "Already connected to darkfid");
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

        Err(Error::Custom("Not connected, is the explorer running in no-sync mode?".to_string()))
    }

    /// Sends a ping request to the client's darkfid endpoint to verify connectivity,
    /// returning `true` if the ping is successful or an error if the request fails.
    async fn ping(&self) -> Result<bool> {
        self.request("ping", &JsonValue::Array(vec![])).await?;
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
    // **Example API Usage:**
    // --> {"jsonrpc": "2.0", "method": "ping_darkfid", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn ping_darkfid(&self, params: &JsonValue) -> Result<JsonValue> {
        // Log the start of the operation
        debug!(target: "explorerd::rpc::ping_darkfid", "Pinging darkfid daemon...");

        // Validate that the parameters are empty
        validate_empty_params(params)?;

        // Attempt to ping the darkfid daemon
        self.darkfid_client
            .ping()
            .await
            .map_err(|e| ExplorerdError::PingDarkfidFailed(e.to_string()))?;

        // Ping succeeded, return a successful Boolean(true) value
        Ok(JsonValue::Boolean(true))
    }
}

/// Auxiliary function that logs RPC request failures by generating a structured log message
/// containing the provided `req_method`, `params`, and `error` details. Constructs a log target
/// specific to the request method, formats the error message by stringifying the JSON parameters
/// and error, and performs the log operation without returning a value.
fn log_request_failure(req_method: &str, params: &JsonValue, error: &JsonError) {
    // Generate the log target based on request
    let log_target = format!("explorerd::rpc::handle_request::{}", req_method);

    // Stringify the params
    let params_stringified = match params.stringify() {
        Ok(params) => params,
        Err(e) => format!("Failed to stringify params: {:?}", e),
    };

    // Stringfy the error
    let error_stringified = match error.stringify() {
        Ok(err_str) => err_str,
        Err(e) => format!("Failed to stringify error: {:?}", e),
    };

    // Format the error message for the log
    let error_message = format!("RPC Request Failure: method: {req_method}, params: {params_stringified}, error: {error_stringified}");

    // Log the error
    error!(target: &log_target, "{}", error_message);
}

/// Test module for validating API functions within this `mod.rs` file. It ensures that the core API
/// functions behave as expected and that they handle invalid parameters properly.
#[cfg(test)]
mod tests {
    use tinyjson::JsonValue;

    use darkfi::rpc::jsonrpc::JsonRequest;

    use super::*;
    use crate::{
        error::ERROR_CODE_PING_DARKFID_FAILED,
        test_utils::{setup, validate_empty_rpc_parameters},
    };

    #[test]
    /// Validates the failure scenario of the `ping_darkfid` JSON-RPC method by sending a request
    /// to a disconnected darkfid endpoint, ensuring the response is an error with the expected
    /// code and message.
    fn test_ping_darkfid_failure() {
        smol::block_on(async {
            // Set up the Explorerd instance
            let explorerd = setup();

            // Prepare a JSON-RPC request for `ping_darkfid`
            let request = JsonRequest {
                id: 1,
                jsonrpc: "2.0",
                method: "ping_darkfid".to_string(),
                params: JsonValue::Array(vec![]),
            };

            // Call `handle_request` on the Explorerd instance
            let response = explorerd.handle_request(request).await;

            // Verify the response is a `JsonError` with the `PingFailed` error code
            match response {
                JsonResult::Error(actual_error) => {
                    let expected_error_code = ERROR_CODE_PING_DARKFID_FAILED;
                    let expected_error_msg = "Ping darkfid failed: Not connected, is the explorer running in no-sync mode?";
                    assert_eq!(actual_error.error.code, expected_error_code);
                    assert_eq!(actual_error.error.message, expected_error_msg);
                }
                _ => panic!("Expected a JSON object for the response, but got something else"),
            }
        });
    }

    /// Tests the `ping_darkfid` method to ensure it correctly handles cases where non-empty parameters
    /// are supplied, returning an expected error response.
    #[test]
    fn test_ping_darkfid_empty_params() {
        smol::block_on(async {
            validate_empty_rpc_parameters(&setup(), "ping_darkfid").await;
        });
    }
}
