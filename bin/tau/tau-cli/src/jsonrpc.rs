use async_std::sync::Arc;

use async_executor::Executor;
use log::debug;
use serde_json::{json, Value};
use url::Url;

use darkfi::{
    rpc::jsonrpc::{self, JsonResult},
    Error, Result,
};

pub struct JsonRpcClient {
    sender: async_channel::Sender<Value>,
    receiver: async_channel::Receiver<JsonResult>,
}

impl JsonRpcClient {
    pub async fn new(url: Url, executor: Arc<Executor<'_>>) -> Result<Self> {
        let (sender, receiver) = jsonrpc::open_channels(&url, executor).await?;
        Ok(Self { sender, receiver })
    }

    async fn request(&self, value: Value) -> Result<Value> {
        self.sender.send(value).await?;
        let reply: JsonResult = self.receiver.recv().await?;

        match reply {
            JsonResult::Resp(r) => {
                debug!(target: "RPC", "<-- {}", serde_json::to_string(&r)?);
                Ok(r.result)
            }

            JsonResult::Err(e) => {
                debug!(target: "RPC", "<-- {}", serde_json::to_string(&e)?);
                Err(Error::JsonRpcError(e.error.message.to_string()))
            }

            JsonResult::Notif(n) => {
                debug!(target: "RPC", "<-- {}", serde_json::to_string(&n)?);
                Err(Error::JsonRpcError("Unexpected reply".to_string()))
            }
        }
    }

    // RPCAPI:
    // Add new task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "add",
    //      "params":
    //          [{
    //          "title": "..",
    //          "desc": "..",
    //          assign: [..],
    //          project: [..],
    //          "due": ..,
    //          "rank": ..
    //          }],
    //      "id": 1
    //      }
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn add(&self, params: Value) -> Result<Value> {
        let req = jsonrpc::request(json!("add"), params);
        self.request(json!(req)).await
    }

    // List tasks
    // --> {"jsonrpc": "2.0", "method": "get_ids", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [task_id, ...], "id": 1}
    pub async fn get_ids(&self, params: Value) -> Result<Value> {
        let req = jsonrpc::request(json!("get_ids"), json!(params));
        self.request(json!(req)).await
    }

    // Update task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "update", "params": [task_id, {"title": "new title"} ], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn update(&self, id: u64, data: Value) -> Result<Value> {
        let req = jsonrpc::request(json!("update"), json!([id, data]));
        self.request(json!(req)).await
    }

    // Set state for a task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "set_state", "params": [task_id, state], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn set_state(&self, id: u64, state: &str) -> Result<Value> {
        let req = jsonrpc::request(json!("set_state"), json!([id, state]));
        self.request(json!(req)).await
    }

    // Set comment for a task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "set_comment", "params": [task_id, comment_content], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn set_comment(&self, id: u64, content: &str) -> Result<Value> {
        let req = jsonrpc::request(json!("set_comment"), json!([id, content]));
        self.request(json!(req)).await
    }

    // Get task by id.
    // --> {"jsonrpc": "2.0", "method": "get_task_by_id", "params": [task_id], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "task", "id": 1}
    pub async fn get_task_by_id(&self, id: u64) -> Result<Value> {
        let req = jsonrpc::request(json!("get_task_by_id"), json!([id]));
        self.request(json!(req)).await
    }
}
