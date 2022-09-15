use serde_json::Value;

use darkfi::rpc::jsonrpc::{ErrorCode::ServerError, JsonError, JsonResult};

pub enum RpcError {
    AmountExceedsLimit = -32107,
    TimeLimitReached = -32108,
    ParseError = -32109,
}

fn to_tuple(e: RpcError) -> (i64, String) {
    let msg = match e {
        RpcError::AmountExceedsLimit => "Amount requested is higher than the faucet limit.",
        RpcError::TimeLimitReached => "Timeout not expired. Try again later.",
        RpcError::ParseError => "Parse error.",
    };

    (e as i64, msg.to_string())
}

pub fn server_error(e: RpcError, id: Value) -> JsonResult {
    let (code, msg) = to_tuple(e);
    JsonError::new(ServerError(code), Some(msg), id).into()
}
