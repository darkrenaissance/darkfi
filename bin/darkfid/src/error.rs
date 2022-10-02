use serde_json::Value;

use darkfi::rpc::jsonrpc::{ErrorCode::ServerError, JsonError, JsonResult};

/// Custom RPC errors available for darkfid.
/// Please sort them sensefully.
pub enum RpcError {
    // Wallet/Key-related errors
    Keygen = -32101,
    KeypairFetch = -32102,
    KeypairNotFound = -32103,
    InvalidKeypair = -32104,
    InvalidAddressParam = -32105,
    DecryptionFailed = -32106,

    // Transaction-related errors
    TxBuildFail = -32110,
    TxBroadcastFail = -32111,
    TxSimulationFail = -32112,

    // State-related errors,
    NotSynced = -32120,
    UnknownSlot = -32121,

    // Parsing errors
    ParseError = -32190,
    NaN = -32191,
    LessThanNegOne = -32192,
}

fn to_tuple(e: RpcError) -> (i64, String) {
    let msg = match e {
        // Wallet/Key-related errors
        RpcError::Keygen => "Failed generating keypair",
        RpcError::KeypairFetch => "Failed fetching keypairs from wallet",
        RpcError::KeypairNotFound => "Keypair not found",
        RpcError::InvalidKeypair => "Invalid keypair",
        RpcError::InvalidAddressParam => "Invalid address parameter",
        RpcError::DecryptionFailed => "Decryption failed",
        // Transaction-related errors
        RpcError::TxBuildFail => "Failed building transaction",
        RpcError::TxBroadcastFail => "Failed broadcasting transaction",
        RpcError::TxSimulationFail => "Failed simulating transaction state change",
        // State-related errors
        RpcError::NotSynced => "Blockchain is not synced",
        RpcError::UnknownSlot => "Did not find slot",
        // Parsing errors
        RpcError::ParseError => "Parse error",
        RpcError::NaN => "Not a number",
        RpcError::LessThanNegOne => "Number cannot be lower than -1",
    };

    (e as i64, msg.to_string())
}

pub fn server_error(e: RpcError, id: Value, msg: Option<&str>) -> JsonResult {
    let (code, default_msg) = to_tuple(e);

    if let Some(message) = msg {
        return JsonError::new(ServerError(code), Some(message.to_string()), id).into()
    }

    JsonError::new(ServerError(code), Some(default_msg), id).into()
}
