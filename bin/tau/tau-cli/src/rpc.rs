use log::error;
use serde_json::json;

use darkfi::{rpc::jsonrpc, Result};

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
        let req = jsonrpc::request(json!("add"), json!([task]));
        let rep = self.rpc_client.request(req).await.or_else(|e| {
            error!("Failed sending `add` request to taud: {}", e);
            return Err(e)
        })?;

        println!("Got reply: {:?}", rep);
        Ok(())
    }

    /// Get all task ids.
    pub async fn get_ids(&self) -> Result<Vec<u64>> {
        let req = jsonrpc::request(json!("get_ids"), json!([]));
        let rep = self.rpc_client.request(req).await.or_else(|e| {
            error!("Failed sending `get_ids` request to taud: {}", e);
            return Err(e)
        })?;

        let mut ret = vec![];
        for i in rep.as_array().unwrap() {
            ret.push(i.as_u64().unwrap());
        }

        Ok(ret)
    }

    /// Update existing task given it's ID and some params.
    pub async fn update(&self, id: u64, task: BaseTask) -> Result<()> {
        let req = jsonrpc::request(json!("update"), json!([id, task]));
        let rep = self.rpc_client.request(req).await.or_else(|e| {
            error!("Failed sending `update` request to taud: {}", e);
            return Err(e)
        })?;

        println!("Got reply: {:?}", rep);
        Ok(())
    }

    /// Set the state for a task.
    pub async fn set_state(&self, id: u64, state: &str) -> Result<()> {
        let req = jsonrpc::request(json!("set_state"), json!([id, state]));
        let rep = self.rpc_client.request(req).await.or_else(|e| {
            error!("Failed sending `set_state` request to taud: {}", e);
            return Err(e)
        })?;

        println!("Got reply: {:?}", rep);
        Ok(())
    }

    /// Set a comment for a task.
    pub async fn set_comment(&self, id: u64, content: &str) -> Result<()> {
        let req = jsonrpc::request(json!("set_comment"), json!([id, content]));
        let rep = self.rpc_client.request(req).await.or_else(|e| {
            error!("Failed sending `set_comment` request to taud: {}", e);
            return Err(e)
        })?;

        println!("Got reply: {:?}", rep);
        Ok(())
    }

    /// Get task data by its ID.
    pub async fn get_task_by_id(&self, id: u64) -> Result<TaskInfo> {
        let req = jsonrpc::request(json!("get_task_by_id"), json!([id]));
        let rep = self.rpc_client.request(req).await.or_else(|e| {
            error!("Error sending `get_task_by_id` request: {}", e);
            return Err(e)
        })?;

        Ok(serde_json::from_value(rep)?)
    }
}
