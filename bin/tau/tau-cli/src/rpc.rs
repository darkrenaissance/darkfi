/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use darkfi::{rpc::jsonrpc::JsonRequest, Result};
use log::debug;
use tinyjson::JsonValue;

use crate::{
    primitives::{BaseTask, State, TaskInfo},
    Tau,
};

impl Tau {
    pub async fn close_connection(&self) {
        self.rpc_client.stop().await
    }

    /// Add a new task.
    pub async fn add(&self, task: BaseTask) -> Result<u32> {
        let mut params = vec![
            JsonValue::String(task.title.clone()),
            JsonValue::Array(task.tags.iter().map(|x| JsonValue::String(x.clone())).collect()),
            JsonValue::String(task.desc.unwrap_or("".to_string())),
            JsonValue::Array(task.assign.iter().map(|x| JsonValue::String(x.clone())).collect()),
            JsonValue::Array(task.project.iter().map(|x| JsonValue::String(x.clone())).collect()),
        ];

        let due = if let Some(num) = task.due {
            JsonValue::String(num.to_string())
        } else {
            JsonValue::Null
        };
        params.push(due);

        let rank =
            if let Some(num) = task.rank { JsonValue::Number(num.into()) } else { JsonValue::Null };
        params.push(rank);

        let req = JsonRequest::new("add", params);
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        Ok(*rep.get::<f64>().unwrap() as u32)
    }

    /// Get current open tasks ids.
    pub async fn get_ids(&self) -> Result<Vec<u32>> {
        let req = JsonRequest::new("get_ids", vec![]);
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);

        let mut ret = vec![];
        for i in rep.get::<Vec<JsonValue>>().unwrap() {
            ret.push(*i.get::<f64>().unwrap() as u32)
        }

        Ok(ret)
    }

    /// Update existing task given it's ID and some params.
    pub async fn update(&self, id: u32, task: BaseTask) -> Result<bool> {
        let mut params = vec![
            JsonValue::String(task.title.clone()),
            JsonValue::Array(task.tags.iter().map(|x| JsonValue::String(x.clone())).collect()),
            JsonValue::String(task.desc.unwrap_or("".to_string())),
            JsonValue::Array(task.assign.iter().map(|x| JsonValue::String(x.clone())).collect()),
            JsonValue::Array(task.project.iter().map(|x| JsonValue::String(x.clone())).collect()),
        ];

        let due = if let Some(num) = task.due {
            JsonValue::String(num.to_string())
        } else {
            JsonValue::Null
        };
        params.push(due);

        let rank =
            if let Some(num) = task.rank { JsonValue::Number(num.into()) } else { JsonValue::Null };
        params.push(rank);

        let req = JsonRequest::new(
            "update",
            vec![JsonValue::Number(id.into()), JsonValue::Array(params)],
        );
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        Ok(*rep.get::<bool>().unwrap())
    }

    /// Set the state for a task.
    pub async fn set_state(&self, id: u32, state: &State) -> Result<bool> {
        let req = JsonRequest::new(
            "set_state",
            vec![JsonValue::Number(id.into()), JsonValue::String(state.to_string())],
        );
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        Ok(*rep.get::<bool>().unwrap())
    }

    /// Set a comment for a task.
    pub async fn set_comment(&self, id: u32, content: &str) -> Result<bool> {
        let req = JsonRequest::new(
            "set_comment",
            vec![JsonValue::Number(id.into()), JsonValue::String(content.to_string())],
        );
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        Ok(*rep.get::<bool>().unwrap())
    }

    /// Get task data by its ID.
    pub async fn get_task_by_id(&self, id: u32) -> Result<TaskInfo> {
        let req = JsonRequest::new("get_task_by_id", vec![JsonValue::Number(id.into())]);
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        let rep = rep.into();
        Ok(rep)
    }

    /// Get month's stopped tasks.
    pub async fn get_stop_tasks(&self, month: Option<u64>) -> Result<Vec<TaskInfo>> {
        let param = if let Some(month) = month {
            JsonValue::String(month.to_string())
        } else {
            JsonValue::Null
        };

        let req = JsonRequest::new("get_stop_tasks", vec![param]);
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        let rep =
            rep.get::<Vec<JsonValue>>().unwrap().iter().map(|x| (*x).clone().into()).collect();
        Ok(rep)
    }

    /// Switch workspace.
    pub async fn switch_ws(&self, workspace: String) -> Result<bool> {
        let req = JsonRequest::new("switch_ws", vec![JsonValue::String(workspace)]);
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        Ok(*rep.get::<bool>().unwrap())
    }

    /// Get current workspace.
    pub async fn get_ws(&self) -> Result<String> {
        let req = JsonRequest::new("get_ws", vec![]);
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        Ok(rep.get::<String>().unwrap().clone())
    }

    /// Export tasks.
    pub async fn export_to(&self, path: String) -> Result<bool> {
        let req = JsonRequest::new("export", vec![JsonValue::String(path)]);
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        Ok(*rep.get::<bool>().unwrap())
    }

    /// Import tasks.
    pub async fn import_from(&self, path: String) -> Result<bool> {
        let req = JsonRequest::new("import", vec![JsonValue::String(path)]);
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        Ok(*rep.get::<bool>().unwrap())
    }
}
