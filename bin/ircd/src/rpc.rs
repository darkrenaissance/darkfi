use async_trait::async_trait;
use log::debug;
use serde_json::{json, Value};
use url::Url;

use darkfi::{
    net,
    rpc::{
        jsonrpc,
        jsonrpc::{ErrorCode, JsonRequest, JsonResult},
        rpcserver::RequestHandler,
    },
};

pub struct JsonRpcInterface {
    pub addr: Url,
    pub p2p: net::P2pPtr,
}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if req.params.as_array().is_none() {
            return jsonrpc::error(ErrorCode::InvalidRequest, None, req.id).into()
        }

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            Some("ping") => self.pong(req.id, req.params).await,
            Some("get_info") => self.get_info(req.id, req.params).await,
            Some(_) | None => jsonrpc::error(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }
}

impl JsonRpcInterface {
    // RPCAPI:
    // Replies to a ping method.
    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 42}
    async fn pong(&self, id: Value, _params: Value) -> JsonResult {
        jsonrpc::response(json!("pong"), id).into()
    }

    // RPCAPI:
    // Retrieves P2P network information.
    // --> {"jsonrpc": "2.0", "method": "get_info", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", result": {"nodeID": [], "nodeinfo": [], "id": 42}
    async fn get_info(&self, id: Value, _params: Value) -> JsonResult {
        let resp = self.p2p.get_info().await;
        jsonrpc::response(resp, id).into()
    }
}
