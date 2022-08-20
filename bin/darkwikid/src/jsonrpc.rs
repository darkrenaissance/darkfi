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
    sender: async_channel::Sender<String>,
    receiver: async_channel::Receiver<Vec<Vec<(String, String)>>>,
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
            Some("dry_run") => self.dry_run(req.id, params).await,
            Some("log") => self.log(req.id, params).await,
            Some(_) | None => return JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        };

        rep
    }
}

impl JsonRpcInterface {
    pub fn new(
        sender: async_channel::Sender<String>,
        receiver: async_channel::Receiver<Vec<Vec<(String, String)>>>,
    ) -> Self {
        Self { sender, receiver }
    }

    // RPCAPI:
    // Update files in ~/darkwiki
    // --> {"jsonrpc": "2.0", "method": "update", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [[(String, String)]], "id": 1}
    async fn update(&self, id: Value, _params: &[Value]) -> JsonResult {
        let res = self.sender.send("update".into()).await.map_err(Error::from);

        if let Err(e) = res {
            error!("Failed to update: {}", e);
            return JsonError::new(ErrorCode::InternalError, None, id).into()
        }

        let response = self.receiver.recv().await.unwrap();
        JsonResponse::new(json!(response), id).into()
    }

    // RPCAPI:
    // Update files in darkwiki (dry_run)
    // --> {"jsonrpc": "2.0", "method": "dry_run", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [[(String, String)]], "id": 1}
    async fn dry_run(&self, id: Value, _params: &[Value]) -> JsonResult {
        let res = self.sender.send("dry_run".into()).await.map_err(Error::from);

        if let Err(e) = res {
            error!("Failed to update(dry run): {}", e);
            return JsonError::new(ErrorCode::InternalError, None, id).into()
        }

        let response = self.receiver.recv().await.unwrap();
        JsonResponse::new(json!(response), id).into()
    }

    // RPCAPI:
    // Show all patches
    // --> {"jsonrpc": "2.0", "method": "log", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [[(String, String)]], "id": 1}
    async fn log(&self, id: Value, _params: &[Value]) -> JsonResult {
        let res = self.sender.send("log".into()).await.map_err(Error::from);

        if let Err(e) = res {
            error!("Failed to show all patches: {}", e);
            return JsonError::new(ErrorCode::InternalError, None, id).into()
        }

        let response = self.receiver.recv().await.unwrap();
        JsonResponse::new(json!(response), id).into()
    }
}
