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

use std::{collections::HashSet, time::Instant};

use async_trait::async_trait;
use log::{debug, error, info};
use smol::lock::MutexGuard;
use tinyjson::JsonValue;

use darkfi::{
    net::P2pPtr,
    rpc::{
        client::RpcChadClient,
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        p2p_method::HandlerP2p,
        server::RequestHandler,
    },
    system::{sleep, StoppableTaskPtr},
    util::time::Timestamp,
    Error, Result,
};

use crate::{
    error::{server_error, RpcError},
    Darkfid,
};

#[async_trait]
#[rustfmt::skip]
impl RequestHandler for Darkfid {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "darkfid::rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            // =====================
            // Miscellaneous methods
            // =====================
            "ping" => self.pong(req.id, req.params).await,
            "clock" => self.clock(req.id, req.params).await,
            "ping_miner" => self.ping_miner(req.id, req.params).await,
            "dnet.switch" => self.dnet_switch(req.id, req.params).await,
            "dnet.subscribe_events" => self.dnet_subscribe_events(req.id, req.params).await,
            // TODO: Make this optional
            "p2p.get_info" => self.p2p_get_info(req.id, req.params).await,

            // ==================
            // Blockchain methods
            // ==================
            "blockchain.get_block" => self.blockchain_get_block(req.id, req.params).await,
            "blockchain.get_tx" => self.blockchain_get_tx(req.id, req.params).await,
            "blockchain.last_known_block" => self.blockchain_last_known_block(req.id, req.params).await,
            "blockchain.best_fork_next_block_height" => self.blockchain_best_fork_next_block_height(req.id, req.params).await,
            "blockchain.block_target" => self.blockchain_block_target(req.id, req.params).await,
            "blockchain.lookup_zkas" => self.blockchain_lookup_zkas(req.id, req.params).await,
            "blockchain.subscribe_blocks" => self.blockchain_subscribe_blocks(req.id, req.params).await,
            "blockchain.subscribe_txs" =>  self.blockchain_subscribe_txs(req.id, req.params).await,
            "blockchain.subscribe_proposals" => self.blockchain_subscribe_proposals(req.id, req.params).await,

            // ===================
            // Transaction methods
            // ===================
            "tx.simulate" => self.tx_simulate(req.id, req.params).await,
            "tx.broadcast" => self.tx_broadcast(req.id, req.params).await,
            "tx.pending" => self.tx_pending(req.id, req.params).await,
            "tx.clean_pending" => self.tx_pending(req.id, req.params).await,
            "tx.calculate_gas" => self.tx_calculate_gas(req.id, req.params).await,

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

impl Darkfid {
    // RPCAPI:
    // Returns current system clock as `u64` (String) timestamp.
    //
    // --> {"jsonrpc": "2.0", "method": "clock", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "1234", "id": 1}
    async fn clock(&self, id: u16, _params: JsonValue) -> JsonResult {
        JsonResponse::new(JsonValue::String(Timestamp::current_time().inner().to_string()), id)
            .into()
    }

    // RPCAPI:
    // Activate or deactivate dnet in the P2P stack.
    // By sending `true`, dnet will be activated, and by sending `false` dnet
    // will be deactivated. Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "dnet_switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn dnet_switch(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_bool() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let switch = params[0].get::<bool>().unwrap();

        if *switch {
            self.p2p.dnet_enable();
        } else {
            self.p2p.dnet_disable();
        }

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Initializes a subscription to p2p dnet events.
    // Once a subscription is established, `darkirc` will send JSON-RPC notifications of
    // new network events to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "dnet.subscribe_events", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "dnet.subscribe_events", "params": [`event`]}
    pub async fn dnet_subscribe_events(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        self.dnet_sub.clone().into()
    }

    // RPCAPI:
    // Pings configured miner daemon for liveness.
    // Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "ping_miner", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "true", "id": 1}
    async fn ping_miner(&self, id: u16, _params: JsonValue) -> JsonResult {
        if let Err(e) = self.ping_miner_daemon().await {
            error!(target: "darkfid::rpc::ping_miner", "Failed to ping miner daemon: {}", e);
            return server_error(RpcError::PingFailed, id, None)
        }
        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    /// Ping configured miner daemon JSON-RPC endpoint.
    pub async fn ping_miner_daemon(&self) -> Result<()> {
        debug!(target: "darkfid::ping_miner_daemon", "Pinging miner daemon...");
        self.miner_daemon_request("ping", &JsonValue::Array(vec![])).await?;
        Ok(())
    }

    /// Auxiliary function to execute a request towards the configured miner daemon JSON-RPC endpoint.
    pub async fn miner_daemon_request(
        &self,
        method: &str,
        params: &JsonValue,
    ) -> Result<JsonValue> {
        let Some(ref rpc_client) = self.rpc_client else { return Err(Error::RpcClientStopped) };
        debug!(target: "darkfid::rpc::miner_daemon_request", "Executing request {} with params: {:?}", method, params);
        let latency = Instant::now();
        let req = JsonRequest::new(method, params.clone());
        let lock = rpc_client.lock().await;
        let rep = lock.client.request(req).await?;
        drop(lock);
        let latency = latency.elapsed();
        debug!(target: "darkfid::rpc::miner_daemon_request", "Got reply: {:?}", rep);
        debug!(target: "darkfid::rpc::miner_daemon_request", "Latency: {:?}", latency);
        Ok(rep)
    }

    /// Auxiliary function to execute a request towards the configured miner daemon JSON-RPC endpoint,
    /// but in case of failure, sleep and retry until connection is re-established.
    pub async fn miner_daemon_request_with_retry(
        &self,
        method: &str,
        params: &JsonValue,
    ) -> JsonValue {
        loop {
            // Try to execute the request using current client
            match self.miner_daemon_request(method, params).await {
                Ok(v) => return v,
                Err(e) => {
                    error!(target: "darkfid::rpc::miner_daemon_request_with_retry", "Failed to execute miner daemon request: {}", e);
                }
            }
            loop {
                // Sleep a bit before retrying
                info!(target: "darkfid::rpc::miner_daemon_request_with_retry", "Sleeping so we can retry later");
                sleep(10).await;
                // Create a new client
                let mut rpc_client = self.rpc_client.as_ref().unwrap().lock().await;
                let Ok(client) =
                    RpcChadClient::new(rpc_client.endpoint.clone(), rpc_client.ex.clone()).await
                else {
                    error!(target: "darkfid::rpc::miner_daemon_request_with_retry", "Failed to initialize miner daemon rpc client, check if minerd is running");
                    drop(rpc_client);
                    continue
                };
                info!(target: "darkfid::rpc::miner_daemon_request_with_retry", "Connection re-established!");
                // Set the new client as the daemon one
                rpc_client.client = client;
                break;
            }
        }
    }
}

impl HandlerP2p for Darkfid {
    fn p2p(&self) -> P2pPtr {
        self.p2p.clone()
    }
}
