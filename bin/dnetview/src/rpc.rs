use darkfi::{
    error::Result,
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
};

use serde_json::{json, Value};
use url::Url;

use crate::error::{DnetViewError, DnetViewResult};

pub struct RpcConnect {
    pub name: String,
    pub rpc_client: RpcClient,
}

impl RpcConnect {
    pub async fn new(url: Url, name: String) -> Result<Self> {
        let rpc_client = RpcClient::new(url).await?;
        Ok(Self { name, rpc_client })
    }

    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 42}
    pub async fn ping(&self) -> Result<Value> {
        let req = JsonRequest::new("ping", json!([]));
        self.rpc_client.request(req).await
    }

    //--> {"jsonrpc": "2.0", "method": "poll", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": {"nodeID": [], "nodeinfo" [], "id": 42}
    pub async fn get_info(&self) -> DnetViewResult<Value> {
        let req = JsonRequest::new("get_info", json!([]));
        match self.rpc_client.request(req).await {
            Ok(req) => Ok(req),
            Err(e) => Err(DnetViewError::Darkfi(e)),
        }
    }
}
