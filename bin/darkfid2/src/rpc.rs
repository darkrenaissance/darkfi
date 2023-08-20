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

use async_trait::async_trait;
use log::debug;
use tinyjson::JsonValue;

use darkfi::{
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::RequestHandler,
    },
    util::time::Timestamp,
};

use crate::Darkfid;

#[async_trait]
impl RequestHandler for Darkfid {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "darkfid::rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            // =====================
            // Miscellaneous methods
            // =====================
            "ping" => return self.pong(req.id, req.params).await,
            "clock" => return self.clock(req.id, req.params).await,
            "sync_dnet_switch" => return self.sync_dnet_switch(req.id, req.params).await,
            "consensus_dnet_switch" => return self.consensus_dnet_switch(req.id, req.params).await,

            // ==================
            // Blockchain methods
            // ==================
            "blockchain.get_slot" => return self.blockchain_get_slot(req.id, req.params).await,
            "blockchain.get_tx" => return self.blockchain_get_tx(req.id, req.params).await,
            "blockchain.last_known_slot" => {
                return self.blockchain_last_known_slot(req.id, req.params).await
            }
            "blockchain.lookup_zkas" => {
                return self.blockchain_lookup_zkas(req.id, req.params).await
            }
            "blockchain.subscribe_blocks" => {
                return self.blockchain_subscribe_blocks(req.id, req.params).await
            }
            "blockchain.subscribe_txs" => {
                return self.blockchain_subscribe_txs(req.id, req.params).await
            }
            "blockchain.subscribe_proposals" => {
                return self.blockchain_subscribe_proposals(req.id, req.params).await
            }

            // ===================
            // Transaction methods
            // ===================
            "tx.simulate" => return self.tx_simulate(req.id, req.params).await,
            "tx.broadcast" => return self.tx_broadcast(req.id, req.params).await,
            "tx.pending" => return self.tx_pending(req.id, req.params).await,
            "tx.clean_pending" => return self.tx_pending(req.id, req.params).await,

            // ==============
            // Invalid method
            // ==============
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }
}

impl Darkfid {
    // RPCAPI:
    // Returns current system clock as `u64` (String) timestamp.
    //
    // --> {"jsonrpc": "2.0", "method": "clock", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "1234", "id": 1}
    async fn clock(&self, id: u16, _params: JsonValue) -> JsonResult {
        JsonResponse::new(JsonValue::String(Timestamp::current_time().0.to_string()), id).into()
    }

    // RPCAPI:
    // Activate or deactivate dnet in the sync P2P stack.
    // By sending `true`, dnet will be activated, and by sending `false` dnet
    // will be deactivated. Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "sync_dnet_switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn sync_dnet_switch(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_bool() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let switch = params[0].get::<bool>().unwrap();

        if *switch {
            self.sync_p2p.dnet_enable().await;
        } else {
            self.sync_p2p.dnet_disable().await;
        }

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Activate or deactivate dnet in the consensus P2P stack.
    // By sending `true`, dnet will be activated, and by sending `false` dnet
    // will be deactivated. Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "consensus_dnet_switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn consensus_dnet_switch(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_bool() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        if self.consensus_p2p.is_some() {
            let switch = params[0].get::<bool>().unwrap();
            if *switch {
                self.consensus_p2p.clone().unwrap().dnet_enable().await;
            } else {
                self.consensus_p2p.clone().unwrap().dnet_disable().await;
            }
        }

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }
}
