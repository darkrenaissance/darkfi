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

use log::debug;
use serde_json::json;

use darkfi::{
    event_graph::model::Event,
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
    Result,
};

use crate::BaseEvent;

pub struct Gen {
    pub rpc_client: RpcClient,
}

impl Gen {
    pub async fn close_connection(&self) -> Result<()> {
        self.rpc_client.close().await
    }

    /// Add a new task.
    pub async fn add(&self, event: BaseEvent) -> Result<()> {
        let req = JsonRequest::new("add", json!([event]));
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        Ok(())
    }

    /// Get current open tasks ids.
    pub async fn list(&self) -> Result<Vec<Event<BaseEvent>>> {
        let req = JsonRequest::new("list", json!([]));
        let rep = self.rpc_client.request(req).await?;

        debug!("reply: {:?}", rep);

        let bytes: Vec<u8> = serde_json::from_value(rep)?;
        let events: Vec<Event<BaseEvent>> = darkfi_serial::deserialize(&bytes)?;

        Ok(events)
    }
}
