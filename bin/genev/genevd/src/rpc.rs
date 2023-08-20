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

use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use log::debug;
use tinyjson::JsonValue;

use darkfi::{
    event_graph::{
        model::{Event, EventId, ModelPtr},
        protocol_event::SeenPtr,
    },
    net,
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::RequestHandler,
    },
    util::{encoding::base64, time::Timestamp},
};
use darkfi_serial::deserialize;
use genevd::GenEvent;

pub struct JsonRpcInterface {
    _nickname: String,
    missed_events: Arc<Mutex<Vec<Event<GenEvent>>>>,
    model: ModelPtr<GenEvent>,
    seen: SeenPtr<EventId>,
    p2p: net::P2pPtr,
}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        match req.method.as_str() {
            "add" => self.add(req.id, req.params).await,
            "list" => self.list(req.id, req.params).await,

            "ping" => self.pong(req.id, req.params).await,
            "dnet_switch" => self.dnet_switch(req.id, req.params).await,
            _ => return JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }
}

impl JsonRpcInterface {
    pub fn new(
        _nickname: String,
        missed_events: Arc<Mutex<Vec<Event<GenEvent>>>>,
        model: ModelPtr<GenEvent>,
        seen: SeenPtr<EventId>,
        p2p: net::P2pPtr,
    ) -> Self {
        Self { _nickname, missed_events, model, seen, p2p }
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
            self.p2p.dnet_enable().await;
        } else {
            self.p2p.dnet_disable().await;
        }

        JsonResponse::new(JsonValue::Boolean(true), id).into()
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
        let dec = base64::decode(&b64).unwrap();
        let genevent: GenEvent = deserialize(&dec).unwrap();

        let event = Event {
            previous_event_hash: self.model.lock().await.get_head_hash(),
            action: genevent,
            timestamp: Timestamp::current_time(),
        };

        if !self.seen.push(&event.hash()).await {
            let json = JsonValue::Boolean(false);
            return JsonResponse::new(json, id).into()
        }

        self.p2p.broadcast(&event).await;

        let json = JsonValue::Boolean(true);
        JsonResponse::new(json, id).into()
    }

    // RPCAPI:
    // List events
    // --> {"jsonrpc": "2.0", "method": "list", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [task_id, ...], "id": 1}
    async fn list(&self, id: u16, _params: JsonValue) -> JsonResult {
        debug!("fetching all events");
        let msd = self.missed_events.lock().await.clone();

        let ser = darkfi_serial::serialize(&msd);
        let enc = JsonValue::String(base64::encode(&ser));

        JsonResponse::new(enc, id).into()
    }
}
