use serde_json::{json, Value};

use darkfi::{rpc::jsonrpc::JsonRequest, Result};

use crate::Rpc;

impl Rpc {
    // --> {"jsonrpc": "2.0", "method": "create", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "creating dao...", "id": 42}
    pub async fn create(
        &self,
        dao_proposer_limit: u64,
        dao_quorum: u64,
        dao_approval_ratio_quot: u64,
        dao_approval_ratio_base: u64,
    ) -> Result<Value> {
        let req = JsonRequest::new(
            "create",
            json!([
                dao_proposer_limit,
                dao_quorum,
                dao_approval_ratio_quot,
                dao_approval_ratio_base,
            ]),
        );
        self.client.request(req).await
    }

    // --> {"jsonrpc": "2.0", "method": "mint", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "minting tokens...", "id": 42}
    pub async fn mint(&self) -> Result<Value> {
        let req = JsonRequest::new("mint", json!([]));
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
