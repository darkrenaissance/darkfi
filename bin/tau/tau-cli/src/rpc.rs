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
use serde_json::{from_value, json};

use darkfi::{rpc::jsonrpc::JsonRequest, Result};

use crate::{
    primitives::{BaseTask, State, TaskInfo},
    Tau,
};

impl Tau {
    pub async fn close_connection(&self) -> Result<()> {
        self.rpc_client.close().await
    }

    /// Add a new task.
    pub async fn add(&self, task: BaseTask) -> Result<bool> {
        let req = JsonRequest::new("add", json!([task]));
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        let reply: bool = from_value(rep)?;
        Ok(reply)
    }

    /// Get current open tasks ids.
    pub async fn get_ids(&self) -> Result<Vec<u64>> {
        let req = JsonRequest::new("get_ids", json!([]));
        let rep = self.rpc_client.request(req).await?;

        let mut ret = vec![];
        for i in rep.as_array().unwrap() {
            ret.push(i.as_u64().unwrap());
        }

        Ok(ret)
    }

    /// Update existing task given it's ID and some params.
    pub async fn update(&self, id: u64, task: BaseTask) -> Result<bool> {
        let req = JsonRequest::new("update", json!([id, task]));
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        let reply: bool = from_value(rep)?;
        Ok(reply)
    }

    /// Set the state for a task.
    pub async fn set_state(&self, id: u64, state: &State) -> Result<bool> {
        let req = JsonRequest::new("set_state", json!([id, state.to_string()]));
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        let reply: bool = from_value(rep)?;
        Ok(reply)
    }

    /// Set a comment for a task.
    pub async fn set_comment(&self, id: u64, content: &str) -> Result<bool> {
        let req = JsonRequest::new("set_comment", json!([id, content]));
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        let reply: bool = from_value(rep)?;
        Ok(reply)
    }

    /// Get task data by its ID.
    pub async fn get_task_by_id(&self, id: u64) -> Result<TaskInfo> {
        let req = JsonRequest::new("get_task_by_id", json!([id]));
        let rep = self.rpc_client.request(req).await?;

        Ok(serde_json::from_value(rep)?)
    }

    /// Get month's stopped tasks.
    pub async fn get_stop_tasks(&self, month: Option<i64>) -> Result<Vec<TaskInfo>> {
        let req = JsonRequest::new("get_stop_tasks", json!([month]));
        let rep = self.rpc_client.request(req).await?;

        Ok(serde_json::from_value(rep)?)
    }

    /// Switch workspace.
    pub async fn switch_ws(&self, workspace: String) -> Result<bool> {
        let req = JsonRequest::new("switch_ws", json!([workspace]));
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        let reply: bool = from_value(rep)?;
        Ok(reply)
    }

    /// Get current workspace.
    pub async fn get_ws(&self) -> Result<String> {
        let req = JsonRequest::new("get_ws", json!([]));
        let rep = self.rpc_client.request(req).await?;

        Ok(serde_json::from_value(rep)?)
    }

    /// Export tasks.
    pub async fn export_to(&self, path: String) -> Result<bool> {
        let req = JsonRequest::new("export", json!([path]));
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);

        Ok(serde_json::from_value(rep)?)
    }

    /// Import tasks.
    pub async fn import_from(&self, path: String) -> Result<bool> {
        let req = JsonRequest::new("import", json!([path]));
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);

        Ok(serde_json::from_value(rep)?)
    }
}
