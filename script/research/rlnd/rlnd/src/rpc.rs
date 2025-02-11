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
use darkfi::{
    rpc::{
        client::RpcClient,
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::RequestHandler,
    },
    system::{sleep, ExecutorPtr, StoppableTaskPtr},
    Error, Result,
};
use darkfi_sdk::{crypto::pasta_prelude::PrimeField, pasta::pallas};
use darkfi_serial::serialize;
use log::{debug, error, info};
use smol::lock::MutexGuard;
use tinyjson::JsonValue;
use url::Url;

use crate::{
    error::{server_error, RpcError},
    RlnNode,
};

/// Private JSON-RPC `RequestHandler` type
pub struct PrivateRpcHandler;
/// Publicly exposed JSON-RPC `RequestHandler` type
pub struct PublicRpcHandler;

/// Structure to hold a JSON-RPC client and its config,
/// so we can recreate it in case of an error.
pub struct DarkircRpcClient {
    endpoint: Url,
    ex: ExecutorPtr,
    client: RpcClient,
}

impl DarkircRpcClient {
    pub async fn new(endpoint: Url, ex: ExecutorPtr) -> Result<Self> {
        let client = RpcClient::new(endpoint.clone(), ex.clone()).await?;
        Ok(Self { endpoint, ex, client })
    }

    /// Stop the client.
    pub async fn stop(&self) {
        self.client.stop().await
    }
}

#[async_trait]
impl RequestHandler<PrivateRpcHandler> for RlnNode {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "rlnd::private_rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            // =====================
            // Miscellaneous methods
            // =====================
            "ping" => {
                <RlnNode as RequestHandler<PrivateRpcHandler>>::pong(self, req.id, req.params).await
            }
            "ping_darkirc" => self.ping_darkirc(req.id, req.params).await,

            // ================
            // Database methods
            // ================
            "add_membership" => self.add_membership(req.id, req.params).await,
            "get_memberships" => self.get_memberships(req.id, req.params).await,
            "slash_membership" => self.slash_membership(req.id, req.params).await,

            // ==============
            // Invalid method
            // ==============
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.private_rpc_connections.lock().await
    }
}

#[async_trait]
impl RequestHandler<PublicRpcHandler> for RlnNode {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "rlnd::public_rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            // =====================
            // Miscellaneous methods
            // =====================
            "ping" => {
                <RlnNode as RequestHandler<PublicRpcHandler>>::pong(self, req.id, req.params).await
            }

            // ================
            // Database methods
            // ================
            "slash_membership" => self.slash_membership(req.id, req.params).await,

            // ==============
            // Invalid method
            // ==============
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.public_rpc_connections.lock().await
    }
}

impl RlnNode {
    // RPCAPI:
    // Pings configured darkirc daemon for liveness.
    // Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "ping_darkirc", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "true", "id": 1}
    async fn ping_darkirc(&self, id: u16, _params: JsonValue) -> JsonResult {
        if let Err(e) = self.ping_darkirc_daemon().await {
            error!(target: "rlnd::rpc::ping_darkirc", "Failed to ping darkirc daemon: {}", e);
            return server_error(RpcError::PingFailed, id, None)
        }
        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    /// Ping configured darkirc daemon JSON-RPC endpoint.
    pub async fn ping_darkirc_daemon(&self) -> Result<()> {
        debug!(target: "rlnd::ping_darkirc_daemon", "Pinging darkirc daemon...");
        self.darkirc_daemon_request("ping", &JsonValue::Array(vec![])).await?;
        Ok(())
    }

    /// Auxiliary function to execute a request towards the configured darkirc daemon JSON-RPC endpoint.
    pub async fn darkirc_daemon_request(
        &self,
        method: &str,
        params: &JsonValue,
    ) -> Result<JsonValue> {
        debug!(target: "rlnd::rpc::darkirc_daemon_request", "Executing request {} with params: {:?}", method, params);
        let latency = Instant::now();
        let req = JsonRequest::new(method, params.clone());
        let lock = self.rpc_client.lock().await;
        let rep = lock.client.request(req).await?;
        drop(lock);
        let latency = latency.elapsed();
        debug!(target: "rlnd::rpc::darkirc_daemon_request", "Got reply: {:?}", rep);
        debug!(target: "rlnd::rpc::darkirc_daemon_request", "Latency: {:?}", latency);
        Ok(rep)
    }

    /// Auxiliary function to execute a request towards the configured darkirc daemon JSON-RPC endpoint,
    /// but in case of failure, sleep and retry until connection is re-established.
    pub async fn darkirc_daemon_request_with_retry(
        &self,
        method: &str,
        params: &JsonValue,
    ) -> JsonValue {
        loop {
            // Try to execute the request using current client
            match self.darkirc_daemon_request(method, params).await {
                Ok(v) => return v,
                Err(e) => {
                    error!(target: "rlnd::rpc::darkirc_daemon_request_with_retry", "Failed to execute darkirc daemon request: {}", e);
                }
            }
            loop {
                // Sleep a bit before retrying
                info!(target: "rlnd::rpc::darkirc_daemon_request_with_retry", "Sleeping so we can retry later");
                sleep(10).await;
                // Create a new client
                let mut rpc_client = self.rpc_client.lock().await;
                let Ok(client) =
                    RpcClient::new(rpc_client.endpoint.clone(), rpc_client.ex.clone()).await
                else {
                    error!(target: "rlnd::rpc::darkirc_daemon_request_with_retry", "Failed to initialize darkirc daemon rpc client, check if darkirc is running");
                    drop(rpc_client);
                    continue
                };
                info!(target: "rlnd::rpc::darkirc_daemon_request_with_retry", "Connection re-established!");
                // Set the new client as the daemon one
                rpc_client.client = client;
                break;
            }
        }
    }

    // RPCAPI:
    // Generate a new membership for given identity and stake.
    // Returns a readable membership upon success.
    //
    // **Params:**
    // * `array[0]`: base58-encoded `pallas::Base` string
    // * `array[1]`: `u64` Membership stake (as string)
    //
    // **Returns:**
    // * `String`: `Membership` struct serialized into base58.
    //
    // --> {"jsonrpc": "2.0", "method": "add_membership", "params": ["3px89oUYY7nzA43vNCBkbvJbsQjNeuH5XRxJ9C2oGnmV", "42"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    async fn add_membership(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 2 || !params[0].is_string() || !params[1].is_string() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let membership_id = match parse_pallas_base(params[0].get::<String>().unwrap()) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rlnd::rpc::add_membership", "Error parsing membership id: {e}");
                return JsonError::new(ErrorCode::InvalidParams, None, id).into()
            }
        };

        let stake = match params[1].get::<String>().unwrap().parse::<u64>() {
            Ok(v) => v,
            Err(_) => return JsonError::new(ErrorCode::ParseError, None, id).into(),
        };

        let membership = match self.database.add_membership(membership_id, stake) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rlnd::rpc::add_membership", "Failed generating membership: {e}");
                return JsonError::new(ErrorCode::InternalError, None, id).into()
            }
        };

        let membership = bs58::encode(&serialize(&membership)).into_string();
        JsonResponse::new(JsonValue::String(membership), id).into()
    }

    // RPCAPI:
    // Returns all database memberships upon success.
    //
    // **Params:**
    // * `None`
    //
    // **Returns:**
    // * `array[N]`: Pairs of `pallas::Base` and `Membership` serialized into base58
    //
    // --> {"jsonrpc": "2.0", "method": "get_memberships", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    async fn get_memberships(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let memberships = match self.database.get_all() {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rlnd::rpc::get_memberships", "Error retrieving memberships: {e}");
                return JsonError::new(ErrorCode::InvalidParams, None, id).into()
            }
        };

        let mut ret = vec![];
        for (id, membership) in memberships {
            ret.push(JsonValue::String(bs58::encode(&id.to_repr()).into_string()));
            ret.push(JsonValue::String(bs58::encode(&serialize(&membership)).into_string()));
        }

        JsonResponse::new(JsonValue::Array(ret), id).into()
    }

    // RPCAPI:
    // Slash the membership of given identity.
    // Returns the membership information upon success.
    //
    // **Params:**
    // * `array[0]`: base58-encoded `pallas::Base` string
    //
    // **Returns:**
    // * `String`: `Membership` struct serialized into base58.
    //
    // --> {"jsonrpc": "2.0", "method": "slash_membership", "params": ["3px89oUYY7nzA43vNCBkbvJbsQjNeuH5XRxJ9C2oGnmV"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    async fn slash_membership(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let membership_id = match parse_pallas_base(params[0].get::<String>().unwrap()) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rlnd::rpc::slash_membership", "Error parsing membership id: {e}");
                return JsonError::new(ErrorCode::InvalidParams, None, id).into()
            }
        };

        let membership = match self.database.remove_membership_by_id(&membership_id) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "rlnd::rpc::slash_membership", "Failed removing membership: {e}");
                return JsonError::new(ErrorCode::InternalError, None, id).into()
            }
        };

        let membership = bs58::encode(&serialize(&membership)).into_string();
        JsonResponse::new(JsonValue::String(membership), id).into()
    }
}

/// Auxiliary function to parse a `pallas::Base` membership id from a `JsonValue::String`.
pub fn parse_pallas_base(id: &str) -> Result<pallas::Base> {
    let Ok(decoded_bytes) = bs58::decode(id).into_vec() else {
        error!(target: "rlnd::rpc::parse_pallas_base", "Error decoding string: {id}");
        return Err(Error::ParseFailed("Invalid pallas::Base"))
    };

    let bytes: [u8; 32] = match decoded_bytes.try_into() {
        Ok(b) => b,
        Err(e) => {
            error!(target: "rlnd::rpc::parse_pallas_base", "Error decoding string bytes: {e:?}");
            return Err(Error::ParseFailed("Invalid pallas::Base"))
        }
    };

    match pallas::Base::from_repr(bytes).into() {
        Some(id) => Ok(id),
        None => {
            error!(target: "rlnd::rpc::parse_pallas_base", "Error converting bytes to pallas::Base");
            Err(Error::ParseFailed("Invalid pallas::Base"))
        }
    }
}
