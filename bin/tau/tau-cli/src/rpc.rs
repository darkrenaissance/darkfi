use log::debug;
use serde_json::json;

use darkfi::{rpc::jsonrpc::JsonRequest, Result};

use crate::{
    primitives::{BaseTask, TaskInfo},
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

    /// Get all task ids.
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
    pub async fn set_state(&self, id: u64, state: &str) -> Result<()> {
        let req = JsonRequest::new("set_state", json!([id, state]));
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
}
