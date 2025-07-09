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

use async_trait::async_trait;
use darkfi::{net::P2pPtr, system::StoppableTaskPtr};
use smol::lock::MutexGuard;
use std::collections::HashSet;
use tracing::debug;

use darkfi::rpc::{
    jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
    p2p_method::HandlerP2p,
    server::RequestHandler,
    util::JsonValue,
};

use crate::{dchatmsg::DchatMsg, Dchat};

#[async_trait]
impl RequestHandler<()> for Dchat {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "dchat::rpc", "--> {}", req.stringify().unwrap());

        // ANCHOR: req_match
        match req.method.as_str() {
            "send" => self.send(req.id, req.params).await,
            "recv" => self.recv(req.id).await,
            "ping" => self.pong(req.id, req.params).await,
            "p2p.get_info" => self.p2p_get_info(req.id, req.params).await,
            "dnet.switch" => self.dnet_switch(req.id, req.params).await,
            "dnet.subscribe_events" => self.dnet_subscribe_events(req.id, req.params).await,
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
        // ANCHOR_END: req_match
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

impl Dchat {
    // RPCAPI:
    // TODO
    // --> {"jsonrpc": "2.0", "method": "send", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn send(&self, id: u16, params: JsonValue) -> JsonResult {
        let msg = params[0].get::<String>().unwrap().to_string();
        let dchatmsg = DchatMsg { msg };
        self.p2p.broadcast(&dchatmsg).await;
        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // TODO
    // --> {"jsonrpc": "2.0", "method": "inbox", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn recv(&self, id: u16) -> JsonResult {
        let buffer = self.recv_msgs.lock().await;
        let msgs: Vec<JsonValue> =
            buffer.iter().map(|x| JsonValue::String(x.msg.clone())).collect();
        JsonResponse::new(JsonValue::Array(msgs), id).into()
    }

    // RPCAPI:
    // Activate or deactivate dnet in the P2P stack.
    // By sending `true`, dnet will be activated, and by sending `false` dnet will
    // be deactivated. Returns `true` on success.
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
    //
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
}

impl HandlerP2p for Dchat {
    fn p2p(&self) -> P2pPtr {
        self.p2p.clone()
    }
}
