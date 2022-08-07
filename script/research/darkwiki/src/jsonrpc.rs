use async_trait::async_trait;

use log::error;
use serde_json::{json, Value};

use darkfi::{
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::RequestHandler,
    },
    Error,
};

pub struct JsonRpcInterface {
    update_notifier: async_channel::Sender<()>,
}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return JsonError::new(ErrorCode::InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        let rep = match req.method.as_str() {
            Some("update") => self.update(req.id, params).await,
            Some(_) | None => return JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        };

        rep
    }
}

impl JsonRpcInterface {
    pub fn new(update_notifier: async_channel::Sender<()>) -> Self {
        Self { update_notifier }
    }

    // RPCAPI:
    // Update files in ~/darkwiki
    // --> {"jsonrpc": "2.0", "method": "update", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn update(&self, id: Value, _params: &[Value]) -> JsonResult {
        let res = self.update_notifier.send(()).await.map_err(Error::from);

        if let Err(e) = res {
            error!("Failed to update: {}", e);
            return JsonError::new(ErrorCode::InternalError, None, id).into()
        }

        JsonResponse::new(json!(true), id).into()
    }
}
