use serde_json::Value;

use darkfi::rpc::jsonrpc::{ErrorCode::ServerError, JsonError, JsonResult};

pub enum RpcError {
    Keygen = -32101,
    Nan = -32102,
    LessThanNegOne = -32103,
    KeypairFetch = -32104,
    KeypairNotFound = -32105,
    InvalidKeypair = -32106,
    UnknownSlot = -32107,
    TxBuildFail = -32108,
    NetworkNameError = -32109,
    ParseError = -32110,
    TxBroadcastFail = -32111,
    NotYetSynced = -32112,
    InvalidAddressParam = -32113,
    InvalidAmountParam = -32114,
}

fn to_tuple(e: RpcError) -> (i64, String) {
    let msg = match e {
        RpcError::Keygen => "Failed generating keypair",
        RpcError::Nan => "Not a number",
        RpcError::LessThanNegOne => "Number cannot be lower than -1",
        RpcError::KeypairFetch => "Failed fetching keypairs from wallet",
        RpcError::KeypairNotFound => "Keypair not found",
        RpcError::InvalidKeypair => "Invalid keypair",
        RpcError::UnknownSlot => "Did not find slot",
        RpcError::TxBuildFail => "Failed building transaction",
        RpcError::NetworkNameError => "Unknown network name",
        RpcError::ParseError => "Parse error",
        RpcError::TxBroadcastFail => "Failed broadcasting transaction",
        RpcError::NotYetSynced => "Blockchain not yet synced",
        RpcError::InvalidAddressParam => "Invalid address parameter",
        RpcError::InvalidAmountParam => "invalid amount parameter",
    };

    (e as i64, msg.to_string())
}

pub fn server_error(e: RpcError, id: Value) -> JsonResult {
    let (code, msg) = to_tuple(e);
    JsonError::new(ServerError(code), Some(msg), id).into()
}
