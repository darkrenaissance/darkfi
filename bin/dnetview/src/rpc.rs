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

use serde_json::{json, Value};
use url::Url;

use darkfi::{
    error::Result,
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
};

use crate::error::{DnetViewError, DnetViewResult};

pub struct RpcConnect {
    pub name: String,
    pub rpc_client: RpcClient,
}

impl RpcConnect {
    pub async fn new(url: Url, name: String) -> Result<Self> {
        let rpc_client = RpcClient::new(url).await?;
        Ok(Self { name, rpc_client })
    }

    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 42}
    pub async fn ping(&self) -> Result<Value> {
        let req = JsonRequest::new("ping", json!([]));
        self.rpc_client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "get_info", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": {"nodeID": [], "nodeinfo" [], "id": 42}
    pub async fn get_info(&self) -> DnetViewResult<Value> {
        let req = JsonRequest::new("get_info", json!([]));
        match self.rpc_client.request(req).await {
            Ok(req) => Ok(req),
            Err(e) => Err(DnetViewError::Darkfi(e)),
        }
    }

    // --> {"jsonrpc": "2.0", "method": "get_consensus_info", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": {"nodeID": [], "nodeinfo" [], "id": 42}
    pub async fn get_consensus_info(&self) -> DnetViewResult<Value> {
        let req = JsonRequest::new("get_consensus_info", json!([]));
        match self.rpc_client.request(req).await {
            Ok(req) => Ok(req),
            Err(e) => Err(DnetViewError::Darkfi(e)),
        }
    }

    // Returns all lilith node spawned networks names with their node addresses.
    // --> {"jsonrpc": "2.0", "method": "spawns", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "{spawns}", "id": 42}
    pub async fn lilith_spawns(&self) -> DnetViewResult<Value> {
        let req = JsonRequest::new("spawns", json!([]));
        match self.rpc_client.request(req).await {
            Ok(req) => Ok(req),
            Err(e) => Err(DnetViewError::Darkfi(e)),
        }
    }
}
