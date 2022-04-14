use serde_json::Value;

use darkfi::rpc::{
    jsonrpc,
    jsonrpc::{ErrorCode::ServerError, JsonResult},
};

pub enum RpcError {
    Keygen = -32101,
    Nan = -32102,
    LessThanNegOne = -32103,
    KeypairFetch = -32104,
    KeypairNotFound = -32105,
    InvalidKeypair = -32106,
}

fn to_tuple(e: RpcError) -> (i64, String) {
    let msg = match e {
        RpcError::Keygen => "Failed generating keypair",
        RpcError::Nan => "Not a number",
        RpcError::LessThanNegOne => "Number cannot be lower than -1",
        RpcError::KeypairFetch => "Failed fetching keypairs from wallet",
        RpcError::KeypairNotFound => "Keypair not found",
        RpcError::InvalidKeypair => "Invalid keypair",
    };

    (e as i64, msg.to_string())
}

pub fn server_error(e: RpcError, id: Value) -> JsonResult {
    let (code, msg) = to_tuple(e);
    jsonrpc::error(ServerError(code), Some(msg), id).into()
}
