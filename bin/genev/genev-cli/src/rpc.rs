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

use darkfi::{
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
    util::encoding::base64,
    Result,
};
use darkfi_serial::{deserialize, serialize};
use genevd::GenEvent;
use log::debug;
use tinyjson::JsonValue;

pub struct Gen {
    pub rpc_client: RpcClient,
}

impl Gen {
    pub async fn close_connection(&self) {
        self.rpc_client.stop().await;
    }

    /// Add a new task.
    pub async fn add(&self, event: GenEvent) -> Result<()> {
        let event = JsonValue::String(base64::encode(&serialize(&event)));

        let req = JsonRequest::new("add", JsonValue::Array([event].to_vec()));
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        Ok(())
    }

    /// Get current open tasks ids.
    pub async fn list(&self) -> Result<Vec<GenEvent>> {
        let req = JsonRequest::new("list", JsonValue::Array([].to_vec()));
        let rep = self.rpc_client.request(req).await?;

        debug!("reply: {:?}", rep);

        let bytes: Vec<u8> = base64::decode(rep.get::<String>().unwrap()).unwrap();
        let events: Vec<GenEvent> = deserialize(&bytes)?;

        Ok(events)
    }
}
