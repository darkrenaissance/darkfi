use async_trait::async_trait;
use log::debug;
use serde_json::{json, Value};

use darkfi::rpc::{
    jsonrpc::{ErrorCode::*, JsonError, JsonRequest, JsonResponse, JsonResult},
    server::RequestHandler,
};

pub struct JsonRpcInterface {}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonError::new(InvalidParams, None, req.id).into()
        }

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            Some("create") => return self.create_dao(req.id, req.params).await,
            Some("airdrop") => return self.airdrop_tokens(req.id, req.params).await,
            Some("propose") => return self.create_proposal(req.id, req.params).await,
            Some("vote") => return self.vote(req.id, req.params).await,
            Some("exec") => return self.execute(req.id, req.params).await,
            Some(_) | None => return JsonError::new(MethodNotFound, None, req.id).into(),
        }
    }
}

impl JsonRpcInterface {
    // --> {"method": "create", "params": []}
    // <-- {"result": "creating dao..."}
    async fn create_dao(&self, id: Value, _params: Value) -> JsonResult {
        JsonResponse::new(json!("creating dao..."), id).into()
    }
    // --> {"method": "airdrop_tokens", "params": []}
    // <-- {"result": "airdropping tokens..."}
    async fn airdrop_tokens(&self, id: Value, _params: Value) -> JsonResult {
        JsonResponse::new(json!("airdropping tokens..."), id).into()
    }
    // --> {"method": "create_proposal", "params": []}
    // <-- {"result": "creating proposal..."}
    async fn create_proposal(&self, id: Value, _params: Value) -> JsonResult {
        JsonResponse::new(json!("creating proposal..."), id).into()
    }
    // --> {"method": "vote", "params": []}
    // <-- {"result": "voting..."}
    async fn vote(&self, id: Value, _params: Value) -> JsonResult {
        JsonResponse::new(json!("voting..."), id).into()
    }
    // --> {"method": "execute", "params": []}
    // <-- {"result": "executing..."}
    async fn execute(&self, id: Value, _params: Value) -> JsonResult {
        JsonResponse::new(json!("executing..."), id).into()
    }
}
