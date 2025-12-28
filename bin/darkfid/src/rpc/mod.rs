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

use async_trait::async_trait;
use smol::lock::MutexGuard;
use tinyjson::JsonValue;
use tracing::debug;

use darkfi::{
    net::P2pPtr,
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        p2p_method::HandlerP2p,
        server::RequestHandler,
    },
    system::StoppableTaskPtr,
    util::time::Timestamp,
};

use crate::DarkfiNode;

/// Blockchain related methods
mod rpc_blockchain;

/// Transactions related methods
mod rpc_tx;

/// Stratum JSON-RPC related methods for native mining
pub mod rpc_stratum;

/// HTTP JSON-RPC related methods for merge mining
pub mod rpc_xmr;

/// Default JSON-RPC `RequestHandler`
pub struct DefaultRpcHandler;

#[async_trait]
#[rustfmt::skip]
impl RequestHandler<DefaultRpcHandler> for DarkfiNode {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "darkfid::rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            // =====================
            // Miscellaneous methods
            // =====================
            "ping" => <DarkfiNode as RequestHandler<DefaultRpcHandler>>::pong(self, req.id, req.params).await,
            "clock" => self.clock(req.id, req.params).await,
            "dnet.switch" => self.dnet_switch(req.id, req.params).await,
            "dnet.subscribe_events" => self.dnet_subscribe_events(req.id, req.params).await,
            "p2p.get_info" => self.p2p_get_info(req.id, req.params).await,

            // ==================
            // Blockchain methods
            // ==================
            "blockchain.get_block" => self.blockchain_get_block(req.id, req.params).await,
            "blockchain.get_tx" => self.blockchain_get_tx(req.id, req.params).await,
            "blockchain.last_confirmed_block" => self.blockchain_last_confirmed_block(req.id, req.params).await,
            "blockchain.best_fork_next_block_height" => self.blockchain_best_fork_next_block_height(req.id, req.params).await,
            "blockchain.block_target" => self.blockchain_block_target(req.id, req.params).await,
            "blockchain.lookup_zkas" => self.blockchain_lookup_zkas(req.id, req.params).await,
            "blockchain.get_contract_state" => self.blockchain_get_contract_state(req.id, req.params).await,
            "blockchain.get_contract_state_key" => self.blockchain_get_contract_state_key(req.id, req.params).await,
            "blockchain.subscribe_blocks" => self.blockchain_subscribe_blocks(req.id, req.params).await,
            "blockchain.subscribe_txs" =>  self.blockchain_subscribe_txs(req.id, req.params).await,
            "blockchain.subscribe_proposals" => self.blockchain_subscribe_proposals(req.id, req.params).await,

            // ===================
            // Transaction methods
            // ===================
            "tx.simulate" => self.tx_simulate(req.id, req.params).await,
            "tx.broadcast" => self.tx_broadcast(req.id, req.params).await,
            "tx.pending" => self.tx_pending(req.id, req.params).await,
            "tx.clean_pending" => self.tx_clean_pending(req.id, req.params).await,
            "tx.calculate_fee" => self.tx_calculate_fee(req.id, req.params).await,

            // TODO: drop
            // =============
            // Miner methods
            // =============
            /*
            "miner.get_current_mining_randomx_key" => self.miner_get_current_mining_randomx_key(req.id, req.params).await,
            "miner.get_header" => self.miner_get_header(req.id, req.params).await,
            "miner.submit_solution" => self.miner_submit_solution(req.id, req.params).await,
            */

            // ==============
            // Invalid method
            // ==============
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

impl DarkfiNode {
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
            self.p2p_handler.p2p.dnet_enable();
        } else {
            self.p2p_handler.p2p.dnet_disable();
        }

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Initializes a subscription to p2p dnet events.
    // Once a subscription is established, `darkfid` will send JSON-RPC notifications of
    // new network events to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "dnet.subscribe_events", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "dnet.subscribe_events", "params": [`event`]}
    pub async fn dnet_subscribe_events(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        self.subscribers.get("dnet").unwrap().clone().into()
    }
}

impl HandlerP2p for DarkfiNode {
    fn p2p(&self) -> P2pPtr {
        self.p2p_handler.p2p.clone()
    }
}
