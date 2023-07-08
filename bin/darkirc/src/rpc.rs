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
use url::Url;

use darkfi::{
    net,
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::RequestHandler,
    },
};

pub struct JsonRpcInterface {
    pub addr: Url,
    pub p2p: net::P2pPtr,
}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonError::new(ErrorCode::InvalidRequest, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            Some("ping") => self.pong(req.id, params).await,

            Some("dnet_switch") => self.dnet_switch(req.id, params).await,
            Some("dnet_info") => self.dnet_info(req.id, params).await,
            Some(_) | None => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }
}

impl JsonRpcInterface {
    // RPCAPI:
    // Replies to a ping method.
    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 42}
    async fn pong(&self, id: Value, _params: &[Value]) -> JsonResult {
        JsonResponse::new(json!("pong"), id).into()
    }

    // RPCAPI:
    // Activate or deactivate dnet in the P2P stack.
    // By sending `true`, dnet will be activated, and by sending `false` dnet
    // will be deactivated. Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "dnet_switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn dnet_switch(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 && params[0].as_bool().is_none() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        if params[0].as_bool().unwrap() {
            self.p2p.dnet_enable().await;
        } else {
            self.p2p.dnet_disable().await;
        }

        JsonResponse::new(json!(true), id).into()
    }

    // RPCAPI:
    // Retrieves P2P network information.
    //
    // --> {"jsonrpc": "2.0", "method": "dnet_info", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", result": {"nodeID": [], "nodeinfo": [], "id": 42}
    async fn dnet_info(&self, id: Value, _params: &[Value]) -> JsonResult {
        let dnet_info = self.p2p.dnet_info().await;
        JsonResponse::new(net::P2p::map_dnet_info(dnet_info), id).into()
    }
}
