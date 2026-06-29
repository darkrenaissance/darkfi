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
use darkfi::{
    event_graph::{util::recreate_from_replayer_log, Event},
    net::P2pPtr,
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        p2p_method::HandlerP2p,
        server::RequestHandler,
        util::{json_map, JsonValue},
    },
    system::StoppableTaskPtr,
};
use darkfi_serial::deserialize_async_partial;
use smol::lock::MutexGuard;
use tracing::debug;

use super::DarkIrc;
use crate::Privmsg;

#[async_trait]
impl RequestHandler<()> for DarkIrc {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "darkirc::rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            "ping" => self.pong(req.id, req.params).await,
            "dnet.switch" => self.dnet_switch(req.id, req.params).await,
            "dnet.subscribe_events" => self.dnet_subscribe_events(req.id, req.params).await,
            "p2p.get_info" => self.p2p_get_info(req.id, req.params).await,

            "deg.switch" => self.deg_switch(req.id, req.params).await,
            "deg.subscribe_events" => self.deg_subscribe_events(req.id, req.params).await,
            "eventgraph.get_info" => self.eg_get_info(req.id, req.params).await,
            "eventgraph.replay" => self.eg_rep_info(req.id, req.params).await,

            "gource.subscribe_events" => self.gource_subscribe_events(req.id, req.params).await,

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
    async fn dnet_switch(&self, id: i64, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
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
    // Initializes a subscription to p2p dnet events.
    // Once a subscription is established, `darkirc` will send JSON-RPC notifications of
    // new network events to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "dnet.subscribe_events", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "dnet.subscribe_events", "params": [`event`]}
    pub async fn dnet_subscribe_events(&self, id: i64, params: JsonValue) -> JsonResult {
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
    pub async fn deg_subscribe_events(&self, id: i64, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        self.deg_sub.clone().into()
    }

    // RPCAPI:
    // Initializes a subscription to the Gource visualization feed.
    // Once a subscription is established, every rotating-DAG event
    // that successfully decodes as a Privmsg is projected to a
    // Gource-shaped record and forwarded to the subscriber.
    //
    // To feed Gource directly, reformat to the pipe-delimited custom
    // log format and pipe it in:
    // ```
    //   ... | jq -r '.params[0]
    //              | "\(.timestamp)|\(.user)|\(.action)|\(.path)"' \
    //       | gource --log-format custom -
    // ```
    //
    // --> {"jsonrpc": "2.0", "method": "gource.subscribe_events", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "gource.subscribe_events", "params": [`event`]}
    pub async fn gource_subscribe_events(&self, id: i64, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        self.gource_sub.clone().into()
    }

    // RPCAPI:
    // Activate or deactivate deg in the EVENTGRAPH.
    // By sending `true`, deg will be activated, and by sending `false` deg
    // will be deactivated. Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "deg.switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn deg_switch(&self, id: i64, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_bool() {
            return JsonError::new(ErrorCode::InvalidParams, None, id).into()
        }

        let switch = params[0].get::<bool>().unwrap();

        if *switch {
            self.event_graph.deg_enable();
        } else {
            self.event_graph.deg_disable();
        }

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Get EVENTGRAPH info.
    //
    // --> {"jsonrpc": "2.0", "method": "deg.switch", "params": [true], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 42}
    async fn eg_get_info(&self, id: i64, params: JsonValue) -> JsonResult {
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
    async fn eg_rep_info(&self, id: i64, params: JsonValue) -> JsonResult {
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

/// Project a single rotating-DAG event to a Gource-shaped record.
///
/// Returns `None` if the event content isn't a [`Privmsg`] or the
/// privmsg's channel field is empty (in which case there's nothing
/// useful to visualize).
pub async fn privmsg_event_to_gource(event: &Event) -> Option<JsonValue> {
    let privmsg: Privmsg = match deserialize_async_partial(event.content()).await {
        Ok((v, _)) => v,
        Err(_) => return None,
    };

    if privmsg.channel.is_empty() {
        return None
    }

    let path = if let Some(name) = privmsg.channel.strip_prefix('#') {
        format!("channels/{name}")
    } else {
        format!("dms/{}", privmsg.channel)
    };

    // Gource's custom log expects Unix seconds, not millis.
    let unix_secs = event.header.timestamp / 1_000;

    Some(json_map([
        ("timestamp", JsonValue::String(unix_secs.to_string())),
        ("user", JsonValue::String(privmsg.nick.clone())),
        // "M" = modify. We always emit "M" because tracking
        // first-touch (which would justify "A") would need
        // cross-event state and gource creates the file on first
        // reference automatically anyway.
        ("action", JsonValue::String("M".into())),
        ("path", JsonValue::String(path)),
    ]))
}
