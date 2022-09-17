use std::sync::Arc;

use async_std::sync::Mutex;
use async_trait::async_trait;
use log::debug;

use serde_json::{json, Value};

use darkfi::rpc::{
    jsonrpc::{ErrorCode::*, JsonError, JsonRequest, JsonResponse, JsonResult},
    server::RequestHandler,
};

use crate::DaoDemo;

pub struct JsonRpcInterface {
    demo: Arc<Mutex<DaoDemo>>,
}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return JsonError::new(InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            Some("create") => return self.create_dao(req.id, params).await,
            Some("mint") => return self.mint_tokens(req.id, params).await,
            Some("airdrop") => return self.airdrop_tokens(req.id, params).await,
            Some("propose") => return self.create_proposal(req.id, params).await,
            Some("vote") => return self.vote(req.id, params).await,
            Some("exec") => return self.execute(req.id, params).await,
            Some(_) | None => return JsonError::new(MethodNotFound, None, req.id).into(),
        }
    }
}

impl JsonRpcInterface {
    pub fn new(demo: DaoDemo) -> Self {
        let demo = Arc::new(Mutex::new(demo));
        Self { demo }
    }

    // TODO: add 3 params: dao_proposer_limit, dao_quorum, dao_approval_ratio
    //
    // --> {"method": "create", "params": []}
    // <-- {"result": "creating dao..."}
    async fn create_dao(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut demo = self.demo.lock().await;
        demo.create().unwrap();
        JsonResponse::new(json!("dao created"), id).into()
    }
    // --> {"method": "mint_tokens", "params": []}
    // <-- {"result": "minting tokens..."}
    async fn mint_tokens(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut demo = self.demo.lock().await;
        demo.mint().unwrap();
        JsonResponse::new(json!("tokens minted"), id).into()
    }
    // --> {"method": "airdrop_tokens", "params": []}
    // <-- {"result": "airdropping tokens..."}
    async fn airdrop_tokens(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut demo = self.demo.lock().await;
        demo.airdrop().unwrap();
        JsonResponse::new(json!("tokens airdropped"), id).into()
    }
    // --> {"method": "create_proposal", "params": []}
    // <-- {"result": "creating proposal..."}
    async fn create_proposal(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut demo = self.demo.lock().await;
        demo.propose().unwrap();
        JsonResponse::new(json!("proposal created"), id).into()
    }
    // --> {"method": "vote", "params": []}
    // <-- {"result": "voting..."}
    async fn vote(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut demo = self.demo.lock().await;
        demo.vote().unwrap();
        JsonResponse::new(json!("voted"), id).into()
    }
    // --> {"method": "execute", "params": []}
    // <-- {"result": "executing..."}
    async fn execute(&self, id: Value, _params: &[Value]) -> JsonResult {
        let mut demo = self.demo.lock().await;
        demo.exec().unwrap();
        JsonResponse::new(json!("executed"), id).into()
    }
}
