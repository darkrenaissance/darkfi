use serde_json::Value;

use darkfi::rpc::{
    jsonrpc,
    jsonrpc::{ErrorCode::ServerError, JsonResult},
};

const ERROR_KEYGEN: i64 = -32101;
const ERROR_NAN: i64 = -32102;
const ERROR_LT1: i64 = -32103;
const ERROR_KP_FETCH: i64 = -32104;

pub fn err_keygen(id: Value) -> JsonResult {
    jsonrpc::error(ServerError(ERROR_KEYGEN), Some("Failed generating keypair".to_string()), id)
        .into()
}

pub fn err_nan(id: Value) -> JsonResult {
    jsonrpc::error(ServerError(ERROR_NAN), Some("Not a number".to_string()), id).into()
}

pub fn err_lt1(id: Value) -> JsonResult {
    jsonrpc::error(ServerError(ERROR_LT1), Some("Number cannot be lower than -1".to_string()), id)
        .into()
}

pub fn err_kp_fetch(id: Value) -> JsonResult {
    jsonrpc::error(
        ServerError(ERROR_KP_FETCH),
        Some("Failed fetching keypairs from wallet".to_string()),
        id,
    )
    .into()
}
