use serde_json::Value;

use darkfi::rpc::jsonrpc::{ErrorCode::ServerError, JsonError, JsonResult};

pub enum RpcError {
    UnknownKey = -35107,
    QueryFailed = -35108,
    RequestBroadcastFail = -35109,
}

fn to_tuple(e: RpcError) -> (i64, String) {
    let msg = match e {
        RpcError::UnknownKey => "Did not find key",
        RpcError::QueryFailed => "Failed to query key",
        RpcError::RequestBroadcastFail => "Failed to broadcast request",
    };

    (e as i64, msg.to_string())
}

pub fn server_error(e: RpcError, id: Value) -> JsonResult {
    let (code, msg) = to_tuple(e);
    JsonError::new(ServerError(code), Some(msg), id).into()
}
