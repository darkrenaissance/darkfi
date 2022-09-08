use serde_json::{json, Value};

use darkfi::{rpc::jsonrpc::JsonRequest, Result};

use crate::Rpc;

impl Rpc {
    // --> {"jsonrpc": "2.0", "method": "create", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "creating dao...", "id": 42}
    pub async fn create(&self) -> Result<Value> {
        let req = JsonRequest::new("create", json!([]));
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "airdrop", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "airdropping tokens...", "id": 42}
    pub async fn airdrop(&self) -> Result<Value> {
        let req = JsonRequest::new("airdrop", json!([]));
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "propose", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "creating proposal...", "id": 42}
    pub async fn propose(&self) -> Result<Value> {
        let req = JsonRequest::new("propose", json!([]));
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "vote", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "voting...", "id": 42}
    pub async fn vote(&self) -> Result<Value> {
        let req = JsonRequest::new("vote", json!([]));
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "exec", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "executing...", "id": 42}
    pub async fn exec(&self) -> Result<Value> {
        let req = JsonRequest::new("exec", json!([]));
        self.client.request(req).await
    }
}
