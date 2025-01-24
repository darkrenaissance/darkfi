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
use log::{debug, error};
use smol::lock::{Mutex, MutexGuard};
use tinyjson::JsonValue;

use darkfi::{
    event_graph::{proto::EventPut, Event, EventGraphPtr},
    net,
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult, JsonSubscriber},
        p2p_method::HandlerP2p,
        server::RequestHandler,
    },
    system::StoppableTaskPtr,
    util::encoding::base64,
};

use darkfi_serial::{deserialize, deserialize_async_partial, serialize_async};
use genevd::GenEvent;

pub struct JsonRpcInterface {
    _nickname: String,
    event_graph: EventGraphPtr,
    p2p: net::P2pPtr,
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    dnet_sub: JsonSubscriber,
    deg_sub: JsonSubscriber,
}

#[async_trait]
impl RequestHandler<()> for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        match req.method.as_str() {
            "add" => self.add(req.id, req.params).await,
            "list" => self.list(req.id, req.params).await,

            "ping" => self.pong(req.id, req.params).await,
            "dnet.subscribe_events" => self.dnet_subscribe_events(req.id, req.params).await,
            "dnet.switch" => self.dnet_switch(req.id, req.params).await,
            "p2p.get_info" => self.p2p_get_info(req.id, req.params).await,

            "deg.switch" => self.deg_switch(req.id, req.params).await,
            "deg.subscribe_events" => self.deg_subscribe_events(req.id, req.params).await,

            "eventgraph.get_info" => self.eg_get_info(req.id, req.params).await,

            _ => return JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

impl HandlerP2p for JsonRpcInterface {
    fn p2p(&self) -> net::P2pPtr {
        self.p2p.clone()
    }
}

impl JsonRpcInterface {
    pub fn new(
        _nickname: String,
        event_graph: EventGraphPtr,
        p2p: net::P2pPtr,
        dnet_sub: JsonSubscriber,
        deg_sub: JsonSubscriber,
    ) -> Self {
        Self {
            _nickname,
            event_graph,
            p2p,
            rpc_connections: Mutex::new(HashSet::new()),
            dnet_sub,
            deg_sub,
        }
    }

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

    // RPCAPI:
    // Initializes a subscription to deg events.
    // Once a subscription is established, apps using eventgraph will send JSON-RPC notifications of
    // new eventgraph events to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "deg.subscribe_events", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "deg.subscribe_events", "params": [`event`]}
    pub async fn deg_subscribe_events(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
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
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_bool() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let switch = params[0].get::<bool>().unwrap();

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
        let params_ = params.get::<Vec<JsonValue>>().unwrap();
        if !params_.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        self.event_graph.eventgraph_info(id, params).await
    }

    // RPCAPI:
    // Add a new event
    // --> {"jsonrpc": "2.0", "method": "add", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [nickname, ...], "id": 1}
    async fn add(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let b64 = params[0].get::<String>().unwrap();
        let dec = base64::decode(b64).unwrap();
        let genevent: GenEvent = deserialize(&dec).unwrap();

        // Build a DAG event and return it.
        let event = Event::new(serialize_async(&genevent).await, &self.event_graph).await;

        if let Err(e) = self.event_graph.dag_insert(&[event.clone()]).await {
            error!("Failed inserting new event to DAG: {}", e);
        } else {
            // Otherwise, broadcast it
            self.p2p.broadcast(&EventPut(event)).await;
        }

        let json = JsonValue::Boolean(true);
        JsonResponse::new(json, id).into()
    }

    // RPCAPI:
    // List events
    // --> {"jsonrpc": "2.0", "method": "list", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [task_id, ...], "id": 1}
    async fn list(&self, id: u16, _params: JsonValue) -> JsonResult {
        debug!("Fetching all events");
        let mut seen_events = vec![];
        let dag_events = self.event_graph.order_events().await;

        for event in dag_events.iter() {
            let event_id = event.id();
            // Try to deserialize it. (Here we skip errors)
            let genevent: GenEvent = match deserialize_async_partial(event.content()).await {
                Ok((v, _)) => v,
                Err(e) => {
                    error!("Failed deserializing incoming event: {}", e);
                    continue
                }
            };

            debug!("Marking event {} as seen", event_id);
            seen_events.push(genevent);
        }

        let ser = darkfi_serial::serialize(&seen_events);
        let enc = JsonValue::String(base64::encode(&ser));

        JsonResponse::new(enc, id).into()
    }
}
