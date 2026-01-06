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

use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use smol::lock::{Mutex, MutexGuard};
use tinyjson::JsonValue;
use tracing::debug;

use darkfi::{
    net::P2pPtr,
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult, JsonSubscriber},
        p2p_method::HandlerP2p,
        server::RequestHandler,
    },
    system::StoppableTaskPtr,
};

use crate::Fud;

pub struct ManagementRpcInterface {
    fud: Arc<Fud>,
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    dnet_sub: JsonSubscriber,
}

#[async_trait]
impl RequestHandler<()> for ManagementRpcInterface {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "fud::rpc::management_rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            "ping" => self.pong(req.id, req.params).await,
            "dnet.switch" => self.dnet_switch(req.id, req.params).await,
            "dnet.subscribe_events" => self.dnet_subscribe_events(req.id, req.params).await,
            "p2p.get_info" => self.p2p_get_info(req.id, req.params).await,
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

impl HandlerP2p for ManagementRpcInterface {
    fn p2p(&self) -> P2pPtr {
        self.fud.p2p.clone()
    }
}

/// Fud management RPC methods
impl ManagementRpcInterface {
    pub fn new(fud: Arc<Fud>, dnet_sub: JsonSubscriber) -> Self {
        Self { fud, rpc_connections: Mutex::new(HashSet::new()), dnet_sub }
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
            self.fud.p2p.dnet_enable();
        } else {
            self.fud.p2p.dnet_disable();
        }

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Initializes a subscription to P2P dnet events.
    // Once a subscription is established, `fud` will send JSON-RPC
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

        self.dnet_sub.clone().into()
    }
}
