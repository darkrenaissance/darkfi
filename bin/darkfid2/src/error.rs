use serde_json::Value;

use darkfi::rpc::{
    jsonrpc,
    jsonrpc::{ErrorCode::ServerError, JsonResult},
};

const ERROR_KEYGEN: i64 = -32101;
const ERROR_NAN: i64 = -32102;
const ERROR_LT1: i64 = -32103;
const ERROR_KP_FETCH: i64 = -32104;
const ERROR_KP_NOT_FOUND: i64 = -32105;
const ERROR_INVALID_KP: i64 = -32106;

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

pub fn err_kp_not_found(id: Value) -> JsonResult {
    jsonrpc::error(ServerError(ERROR_KP_NOT_FOUND), Some("Keypair not found".to_string()), id)
        .into()
}

pub fn err_invalid_kp(id: Value) -> JsonResult {
    jsonrpc::error(ServerError(ERROR_INVALID_KP), Some("Invalid keypair".to_string()), id).into()
}
