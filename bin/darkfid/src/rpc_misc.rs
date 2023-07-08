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

use serde_json::{json, Value};

use darkfi::{
    net::P2p,
    rpc::jsonrpc::{ErrorCode, JsonError, JsonResponse, JsonResult},
    util::time::Timestamp,
};

use super::Darkfid;

impl Darkfid {
    // RPCAPI:
    // Returns a `pong` to the `ping` request.
    //
    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 1}
    pub async fn misc_pong(&self, id: Value, _params: &[Value]) -> JsonResult {
        JsonResponse::new(json!("pong"), id).into()
    }

    // RPCAPI:
    // Returns current system clock in `Timestamp` format.
    //
    // --> {"jsonrpc": "2.0", "method": "clock", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn misc_clock(&self, id: Value, _params: &[Value]) -> JsonResult {
        JsonResponse::new(json!(Timestamp::current_time()), id).into()
    }

    // RPCAPI:
    // Activate or deactivate dnet in the sync P2P stack.
    // By sending `true`, dnet will be activated, and by sending `false` dnet
    // will be deactivated. Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "sync_dnet_switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    pub async fn misc_sync_dnet_switch(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 && params[0].as_bool().is_none() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        // FIXME: Unwrapping here because lazy

        if params[0].as_bool().unwrap() {
            self.sync_p2p.as_ref().unwrap().dnet_enable().await;
        } else {
            self.sync_p2p.as_ref().unwrap().dnet_disable().await;
        }

        JsonResponse::new(json!(true), id).into()
    }

    // RPCAPI:
    // Returns sync P2P network information.
    //
    // --> {"jsonrpc": "2.0", "method": "sync_dnet_info", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", result": {"nodeID": [], "nodeinfo": [], "id": 42}
    pub async fn misc_sync_dnet_info(&self, id: Value, _params: &[Value]) -> JsonResult {
        let resp = match &self.sync_p2p {
            Some(p2p) => P2p::map_dnet_info(p2p.dnet_info().await),
            None => json!([]),
        };
        JsonResponse::new(resp, id).into()
    }

    // RPCAPI:
    // Activate or deactivate dnet in the consensus P2P stack.
    // By sending `true`, dnet will be activated, and by sending `false` dnet
    // will be deactivated. Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "consensus_dnet_switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    pub async fn misc_consensus_dnet_switch(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 && params[0].as_bool().is_none() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        // FIXME: Unwrapping here because lazy

        if params[0].as_bool().unwrap() {
            self.consensus_p2p.as_ref().unwrap().dnet_enable().await;
        } else {
            self.consensus_p2p.as_ref().unwrap().dnet_disable().await;
        }

        JsonResponse::new(json!(true), id).into()
    }

    // RPCAPI:
    // Returns consensus P2P network information.
    //
    // --> {"jsonrpc": "2.0", "method": "consensus_dnet_info", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", result": {"nodeID": [], "nodeinfo": [], "id": 42}
    pub async fn misc_consensus_dnet_info(&self, id: Value, _params: &[Value]) -> JsonResult {
        let resp = match &self.consensus_p2p {
            Some(p2p) => P2p::map_dnet_info(p2p.dnet_info().await),
            None => json!([]),
        };
        JsonResponse::new(resp, id).into()
    }
}
