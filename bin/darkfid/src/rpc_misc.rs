use serde_json::{json, Value};

use darkfi::{
    rpc::jsonrpc::{JsonResponse, JsonResult},
    util::time::Timestamp,
};

use super::Darkfid;

impl Darkfid {
    // RPCAPI:
    // Returns a `pong` to the `ping` request.
    //
    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 1}
    pub async fn misc_pong(&self, id: Value, _params: &[Value]) -> JsonResult {
        JsonResponse::new(json!("pong"), id).into()
    }

    // RPCAPI:
    // Returns current system clock in `Timestamp` format.
    //
    // --> {"jsonrpc": "2.0", "method": "clock", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn misc_clock(&self, id: Value, _params: &[Value]) -> JsonResult {
        JsonResponse::new(json!(Timestamp::current_time()), id).into()
    }
}
