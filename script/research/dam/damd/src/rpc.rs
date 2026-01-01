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

use std::collections::HashSet;

use async_trait::async_trait;
use smol::lock::MutexGuard;
use tracing::debug;

use darkfi::{
    net::P2pPtr,
    rpc::{
        jsonrpc::{
            ErrorCode::{InvalidParams, MethodNotFound, ParseError},
            JsonError, JsonRequest, JsonResponse, JsonResult,
        },
        p2p_method::HandlerP2p,
        server::RequestHandler,
        util::JsonValue,
    },
    system::StoppableTaskPtr,
};

use crate::DamNode;

#[async_trait]
impl RequestHandler<()> for DamNode {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "damd::rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            // =====================
            // Miscellaneous methods
            // =====================
            "ping" => self.pong(req.id, req.params).await,
            "dnet.switch" => self.dnet_switch(req.id, req.params).await,
            "dnet.subscribe_events" => self.dnet_subscribe_events(req.id, req.params).await,
            "p2p.get_info" => self.p2p_get_info(req.id, req.params).await,

            // =================
            // Protocols methods
            // =================
            "protocols.subscribe_foo" => self.protocols_subscribe_foo(req.id, req.params).await,
            "protocols.subscribe_attack_foo" => {
                self.protocols_subscribe_attack_foo(req.id, req.params).await
            }
            "protocols.subscribe_bar" => self.protocols_subscribe_bar(req.id, req.params).await,
            "protocols.subscribe_attack_bar" => {
                self.protocols_subscribe_attack_bar(req.id, req.params).await
            }

            // =============
            // Flood control
            // =============
            "flood.switch" => self.flood_switch(req.id, req.params).await,

            // ==============
            // Invalid method
            // ==============
            _ => JsonError::new(MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

impl DamNode {
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
            return JsonError::new(InvalidParams, None, id).into()
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
    // Once a subscription is established, `damd` will send JSON-RPC notifications of
    // new network events to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "protocols.subscribe_foo", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "protocols.subscribe_foo", "params": [`event`]}
    pub async fn dnet_subscribe_events(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        self.subscribers.get("dnet").unwrap().clone().into()
    }

    // RPCAPI:
    // Initializes a subscription to new incoming `Foo` messages.
    // Once a subscription is established, `damd` will send JSON-RPC notifications of
    // new incoming `Foo` messages to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "protocols.subscribe_foo", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "protocols.subscribe_foo", "params": [`message`]}
    pub async fn protocols_subscribe_foo(&self, id: u16, params: JsonValue) -> JsonResult {
        self.get_subscriber(id, params, "foo").await
    }

    // RPCAPI:
    // Initializes a subscription to new outgoing attack `Foo` messages.
    // Once a subscription is established, `damd` will send JSON-RPC notifications of
    // new outgoing attack `Foo` messages to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "protocols.subscribe_attack_foo", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "protocols.subscribe_attack_foo", "params": [`message`]}
    pub async fn protocols_subscribe_attack_foo(&self, id: u16, params: JsonValue) -> JsonResult {
        self.get_subscriber(id, params, "attack_foo").await
    }

    // RPCAPI:
    // Initializes a subscription to new incoming `Bar` messages.
    // Once a subscription is established, `damd` will send JSON-RPC notifications of
    // new incoming `Bar` messages to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "protocols.subscribe_bar", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "protocols.subscribe_bar", "params": [`message`]}
    pub async fn protocols_subscribe_bar(&self, id: u16, params: JsonValue) -> JsonResult {
        self.get_subscriber(id, params, "bar").await
    }

    // RPCAPI:
    // Initializes a subscription to new outgoing attack `Bar` messages.
    // Once a subscription is established, `damd` will send JSON-RPC notifications of
    // new outgoing attack `Bar` messages to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "protocols.subscribe_attack_bar", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "protocols.subscribe_attack_bar", "params": [`message`]}
    pub async fn protocols_subscribe_attack_bar(&self, id: u16, params: JsonValue) -> JsonResult {
        self.get_subscriber(id, params, "attack_bar").await
    }

    async fn get_subscriber(&self, id: u16, params: JsonValue, sub: &str) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        self.subscribers.get(sub).unwrap().clone().into()
    }

    // RPCAPI:
    // Activate or deactivate damd flooder.
    // By sending `true`, flooder will be activated, and by sending `false` flooder
    // will be deactivated. Returns `true` on success.
    // A limit can be passed, defining after how many messages the flooder will
    // stop. If its 0, flooder will keep going.
    //
    // --> {"jsonrpc": "2.0", "method": "flood", "params": [true, "100"], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn flood_switch(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 2 || !params[0].is_bool() || !params[1].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let switch = params[0].get::<bool>().unwrap();
        let limit = match params[1].get::<String>().unwrap().parse::<u32>() {
            Ok(v) => v,
            Err(_) => return JsonError::new(ParseError, None, id).into(),
        };

        if *switch {
            self.flooder.start(&self.subscribers, limit).await;
        } else {
            self.flooder.stop().await;
        }

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }
}

impl HandlerP2p for DamNode {
    fn p2p(&self) -> P2pPtr {
        self.p2p_handler.p2p.clone()
    }
}
