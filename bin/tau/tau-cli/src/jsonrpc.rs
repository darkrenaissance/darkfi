use serde_json::{json, Value};

use darkfi::{
    rpc::{jsonrpc, rpcclient::RpcClient},
    Result,
};

pub struct Rpc {
    pub client: RpcClient,
}

impl Rpc {
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
        self.client.request(req).await
    }

    // List tasks
    // --> {"jsonrpc": "2.0", "method": "get_ids", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [task_id, ...], "id": 1}
    pub async fn get_ids(&self, params: Value) -> Result<Value> {
        let req = jsonrpc::request(json!("get_ids"), json!(params));
        self.client.request(req).await
    }

    // Update task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "update", "params": [task_id, {"title": "new title"} ], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn update(&self, id: u64, data: Value) -> Result<Value> {
        let req = jsonrpc::request(json!("update"), json!([id, data]));
        self.client.request(req).await
    }

    // Set state for a task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "set_state", "params": [task_id, state], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn set_state(&self, id: u64, state: &str) -> Result<Value> {
        let req = jsonrpc::request(json!("set_state"), json!([id, state]));
        self.client.request(req).await
    }

    // Set comment for a task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "set_comment", "params": [task_id, comment_content], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn set_comment(&self, id: u64, content: &str) -> Result<Value> {
        let req = jsonrpc::request(json!("set_comment"), json!([id, content]));
        self.client.request(req).await
    }

    // Get task by id.
    // --> {"jsonrpc": "2.0", "method": "get_task_by_id", "params": [task_id], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "task", "id": 1}
    pub async fn get_task_by_id(&self, id: u64) -> Result<Value> {
        let req = jsonrpc::request(json!("get_task_by_id"), json!([id]));
        self.client.request(req).await
    }
}
