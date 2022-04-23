use serde_json::{json, Value};

use darkfi::rpc::{jsonrpc, jsonrpc::JsonResult};

use super::Darkfid;

impl Darkfid {
    // RPCAPI:
    // Returns a `pong` to the `ping` request.
    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 1}
    pub async fn pong(&self, id: Value, _params: &[Value]) -> JsonResult {
        jsonrpc::response(json!("pong"), id).into()
    }
}
