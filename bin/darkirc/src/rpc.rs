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
use darkfi::{
    event_graph::util::recreate_from_replayer_log,
    net::P2pPtr,
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        p2p_method::HandlerP2p,
        server::RequestHandler,
        util::JsonValue,
    },
    system::StoppableTaskPtr,
};
use smol::lock::MutexGuard;
use tracing::debug;

use super::DarkIrc;

#[async_trait]
impl RequestHandler<()> for DarkIrc {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "darkirc::rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            "ping" => self.pong(req.id, req.params).await,
            "dnet.switch" => self.dnet_switch(req.id, req.params).await,
            "dnet.subscribe_events" => self.dnet_subscribe_events(req.id, req.params).await,
            "p2p.get_info" => self.p2p_get_info(req.id, req.params).await,
            "p2p.set_outbound_connections" => {
                self.set_outbound_connections(req.id, req.params).await
            }

            "deg.switch" => self.deg_switch(req.id, req.params).await,
            "deg.subscribe_events" => self.deg_subscribe_events(req.id, req.params).await,
            "eventgraph.get_info" => self.eg_get_info(req.id, req.params).await,
            "eventgraph.replay" => self.eg_rep_info(req.id, req.params).await,

            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

impl DarkIrc {
    // RPCAPI:
    // Activate or deactivate dnet in the P2P stack.
    // By sending `true`, dnet will be activated, and by sending `false` dnet
    // will be deactivated. Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "dnet.switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn dnet_switch(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_bool() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let Some(switch) = params[0].get::<bool>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        if *switch {
            self.p2p.dnet_enable();
        } else {
            self.p2p.dnet_disable();
        }

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Set the number of outbound connections for the P2P stack.
    // Takes a positive integer representing the desired number of outbound connection slots.
    // Returns `true` on success. If the number is greater than current, new slots are added.
    // If the number is less than current, slots are removed (prioritizing empty slots).
    //
    // --> {"jsonrpc": "2.0", "method": "p2p.set_outbound_connections", "params": [5], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn set_outbound_connections(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_number() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let Some(n_f64) = params[0].get::<f64>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        let n = *n_f64 as u32;

        if *n_f64 != n as f64 || n == 0 {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        if let Err(e) = self.p2p.session_outbound().set_outbound_connections(n as usize).await {
            return JsonError::new(ErrorCode::InternalError, Some(e.to_string()), id).into()
        }

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Initializes a subscription to p2p dnet events.
    // Once a subscription is established, `darkirc` will send JSON-RPC notifications of
    // new network events to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "dnet.subscribe_events", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "dnet.subscribe_events", "params": [`event`]}
    pub async fn dnet_subscribe_events(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        self.dnet_sub.clone().into()
    }

    // RPCAPI:
    // Initializes a subscription to deg events.
    // Once a subscription is established, apps using eventgraph will send JSON-RPC notifications of
    // new eventgraph events to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "deg.subscribe_events", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "deg.subscribe_events", "params": [`event`]}
    pub async fn deg_subscribe_events(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        self.deg_sub.clone().into()
    }

    // RPCAPI:
    // Activate or deactivate deg in the EVENTGRAPH.
    // By sending `true`, deg will be activated, and by sending `false` deg
    // will be deactivated. Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "deg.switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn deg_switch(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_bool() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let Some(switch) = params[0].get::<bool>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };

        if *switch {
            self.event_graph.deg_enable().await;
        } else {
            self.event_graph.deg_disable().await;
        }

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Get EVENTGRAPH info.
    //
    // --> {"jsonrpc": "2.0", "method": "deg.switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn eg_get_info(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params_) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if !params_.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        self.event_graph.eventgraph_info(id, params).await
    }

    // RPCAPI:
    // Get replayed EVENTGRAPH info.
    //
    // --> {"jsonrpc": "2.0", "method": "eventgraph.replay", "params": ..., "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn eg_rep_info(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params_) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if !params_.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        recreate_from_replayer_log(&self.replay_datastore).await
    }
}

impl HandlerP2p for DarkIrc {
    fn p2p(&self) -> P2pPtr {
        self.p2p.clone()
    }
}
