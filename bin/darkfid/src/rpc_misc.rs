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

use tinyjson::JsonValue;

use darkfi::{
    rpc::jsonrpc::{ErrorCode, JsonError, JsonResponse, JsonResult},
    util::time::Timestamp,
};

use super::Darkfid;

impl Darkfid {
    // RPCAPI:
    // Returns current system clock as u64 (string) timestamp
    //
    // --> {"jsonrpc": "2.0", "method": "clock", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "1234"}, "id": 1}
    pub async fn misc_clock(&self, id: u16, _params: JsonValue) -> JsonResult {
        JsonResponse::new(JsonValue::String(Timestamp::current_time().0.to_string()), id).into()
    }

    // RPCAPI:
    // Activate or deactivate dnet in the sync P2P stack.
    // By sending `true`, dnet will be activated, and by sending `false` dnet
    // will be deactivated. Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "sync_dnet_switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    pub async fn misc_sync_dnet_switch(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_bool() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let switch = params[0].get::<bool>().unwrap();

        if *switch {
            self.sync_p2p.as_ref().unwrap().dnet_enable().await;
        } else {
            self.sync_p2p.as_ref().unwrap().dnet_disable().await;
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
    pub async fn misc_consensus_dnet_switch(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_bool() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let switch = params[0].get::<bool>().unwrap();

        if *switch {
            self.consensus_p2p.as_ref().unwrap().dnet_enable().await;
        } else {
            self.consensus_p2p.as_ref().unwrap().dnet_disable().await;
        }

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }
}
