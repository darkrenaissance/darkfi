/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use crate::DarkfiNode;

impl DarkfiNode {
    // RPCAPI:
    // Returns current system clock as a UNIX timestamp.
    //
    // --> {"jsonrpc": "2.0", "method": "clock", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": 1767015913, "id": 1}
    pub async fn clock(&self, id: u16, _params: JsonValue) -> JsonResult {
        JsonResponse::new((Timestamp::current_time().inner() as f64).into(), id).into()
    }

    // RPCAPI:
    // Activate or deactivate dnet in the P2P stack.
    // By sending `true`, dnet will be activated, and by sending `false` dnet
    // will be deactivated.
    //
    // Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "dnet.switch", "params": [true], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn dnet_switch(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
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
    // Initializes a subscription to P2P dnet events.
    // Once a subscription is established, `darkfid` will send JSON-RPC
    // notifications of new network events to the subscriber.
    //
    // --> {
    //       "jsonrpc": "2.0",
    //       "method": "dnet.subscribe_events",
    //       "params": [],
    //       "id": 1
    //     }
    // <-- {
    //       "jsonrpc": "2.0",
    //       "method": "dnet.subscribe_events",
    //       "params": [
    //         {
    //           "chan": {"Channel": "Info"},
    //           "cmd": "command",
    //           "time": 1767016282
    //         }
    //       ]
    //     }
    pub async fn dnet_subscribe_events(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        self.subscribers.get("dnet").unwrap().clone().into()
    }
}
