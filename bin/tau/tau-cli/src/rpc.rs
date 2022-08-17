use log::debug;
use serde_json::json;

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
    pub async fn add(&self, task: BaseTask) -> Result<()> {
        let req = JsonRequest::new("add", json!([task]));
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        Ok(())
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
    pub async fn update(&self, id: u64, task: BaseTask) -> Result<()> {
        let req = JsonRequest::new("update", json!([id, task]));
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        Ok(())
    }

    /// Set the state for a task.
    pub async fn set_state(&self, id: u64, state: &State) -> Result<()> {
        let req = JsonRequest::new("set_state", json!([id, state.to_string()]));
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        Ok(())
    }

    /// Set a comment for a task.
    pub async fn set_comment(&self, id: u64, content: &str) -> Result<()> {
        let req = JsonRequest::new("set_comment", json!([id, content]));
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);
        Ok(())
    }

    /// Get task data by its ID.
    pub async fn get_task_by_id(&self, id: u64) -> Result<TaskInfo> {
        let req = JsonRequest::new("get_task_by_id", json!([id]));
        let rep = self.rpc_client.request(req).await?;

        Ok(serde_json::from_value(rep)?)
    }

    /// Get month's stopped tasks.
    pub async fn get_stop_tasks(&self, month: i64) -> Result<Vec<TaskInfo>> {
        let req = JsonRequest::new("get_stop_tasks", json!([month]));
        let rep = self.rpc_client.request(req).await?;

        Ok(serde_json::from_value(rep)?)
    }

    /// Switch workspace.
    pub async fn switch_ws(&self, workspace: String) -> Result<()> {
        let req = JsonRequest::new("switch_ws", json!([workspace]));
        let rep = self.rpc_client.request(req).await?;

        debug!("Got reply: {:?}", rep);

        Ok(())
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
