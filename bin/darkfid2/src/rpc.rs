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
use serde_json::{json, Value};

use darkfi::{
    net,
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
        if req.params.as_array().is_none() {
            return JsonError::new(ErrorCode::InvalidRequest, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            // =====================
            // Miscellaneous methods
            // =====================
            Some("ping") => return self.pong(req.id, params).await,
            Some("clock") => return self.clock(req.id, params).await,
            Some("sync_dnet_switch") => return self.sync_dnet_switch(req.id, params).await,
            Some("sync_dnet_info") => return self.sync_dnet_info(req.id, params).await,
            Some("consensus_dnet_switch") => {
                return self.consensus_dnet_switch(req.id, params).await
            }
            Some("consensus_dnet_info") => return self.consensus_dnet_info(req.id, params).await,

            // ==================
            // Blockchain methods
            // ==================
            Some("blockchain.get_slot") => return self.blockchain_get_slot(req.id, params).await,
            Some("blockchain.get_tx") => return self.blockchain_get_tx(req.id, params).await,
            Some("blockchain.last_known_slot") => {
                return self.blockchain_last_known_slot(req.id, params).await
            }
            Some("blockchain.lookup_zkas") => {
                return self.blockchain_lookup_zkas(req.id, params).await
            }

            // ==============
            // Invalid method
            // ==============
            Some(_) | None => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }
}

impl Darkfid {
    // RPCAPI:
    // Replies to a ping method.
    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 42}
    async fn pong(&self, id: Value, _params: &[Value]) -> JsonResult {
        JsonResponse::new(json!("pong"), id).into()
    }

    // RPCAPI:
    // Returns current system clock in `Timestamp` format.
    //
    // --> {"jsonrpc": "2.0", "method": "clock", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    async fn clock(&self, id: Value, _params: &[Value]) -> JsonResult {
        JsonResponse::new(json!(Timestamp::current_time()), id).into()
    }

    // RPCAPI:
    // Activate or deactivate dnet in the sync P2P stack.
    // By sending `true`, dnet will be activated, and by sending `false` dnet
    // will be deactivated. Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "sync_dnet_switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn sync_dnet_switch(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 && params[0].as_bool().is_none() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        if params[0].as_bool().unwrap() {
            self.sync_p2p.dnet_enable().await;
        } else {
            self.sync_p2p.dnet_disable().await;
        }

        JsonResponse::new(json!(true), id).into()
    }

    // RPCAPI:
    // Retrieves sync P2P network information.
    //
    // --> {"jsonrpc": "2.0", "method": "sync_dnet_info", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", result": {"nodeID": [], "nodeinfo": [], "id": 42}
    async fn sync_dnet_info(&self, id: Value, _params: &[Value]) -> JsonResult {
        let dnet_info = self.sync_p2p.dnet_info().await;
        JsonResponse::new(net::P2p::map_dnet_info(dnet_info), id).into()
    }

    // RPCAPI:
    // Activate or deactivate dnet in the consensus P2P stack.
    // By sending `true`, dnet will be activated, and by sending `false` dnet
    // will be deactivated. Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "consensus_dnet_switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn consensus_dnet_switch(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 && params[0].as_bool().is_none() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        if self.consensus_p2p.is_some() {
            if params[0].as_bool().unwrap() {
                self.consensus_p2p.clone().unwrap().dnet_enable().await;
            } else {
                self.consensus_p2p.clone().unwrap().dnet_disable().await;
            }
        }

        JsonResponse::new(json!(true), id).into()
    }

    // RPCAPI:
    // Retrieves consensus P2P network information.
    //
    // --> {"jsonrpc": "2.0", "method": "consensus_dnet_info", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", result": {"nodeID": [], "nodeinfo": [], "id": 42}
    async fn consensus_dnet_info(&self, id: Value, _params: &[Value]) -> JsonResult {
        let dnet_info = if self.consensus_p2p.is_some() {
            self.consensus_p2p.clone().unwrap().dnet_info().await
        } else {
            vec![]
        };
        JsonResponse::new(net::P2p::map_dnet_info(dnet_info), id).into()
    }
}
