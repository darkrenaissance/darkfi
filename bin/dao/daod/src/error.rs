use serde_json::Value;

use darkfi::rpc::jsonrpc::{ErrorCode::ServerError, JsonError, JsonResult};

#[derive(Debug, thiserror::Error)]
pub enum DaoError {
    #[error("No Proposals found")]
    NoProposals,
    #[error("No DAO params found")]
    DaoNotConfigured,
    #[error("State transition failed: '{0}'")]
    StateTransitionFailed(String),
    #[error("Wallet does not exist")]
    NoWalletFound,
    #[error("State not found")]
    StateNotFound,
    #[error("InternalError")]
    Darkfi(#[from] darkfi::error::Error),
    #[error("Verify proof failed: '{0}', '{0}'")]
    VerifyProofFailed(usize, String),
}

pub type DaoResult<T> = std::result::Result<T, DaoError>;

pub enum RpcError {
    Vote = -32101,
    Propose = -32102,
    Exec = -32103,
    Airdrop = -32104,
    Mint = -32105,
    Keygen = -32106,
    Create = -32107,
    Parse = -32108,
    Balance = -32109,
}

fn to_tuple(e: RpcError) -> (i64, String) {
    let msg = match e {
        RpcError::Vote => "Failed to cast a Vote",
        RpcError::Propose => "Failed to generate a Proposal",
        RpcError::Airdrop => "Failed to transfer an airdrop",
        RpcError::Keygen => "Failed to generate keypair",
        RpcError::Create => "Failed to create DAO",
        RpcError::Exec => "Failed to execute Proposal",
        RpcError::Mint => "Failed to mint DAO treasury",
        RpcError::Parse => "Generic parsing error",
        RpcError::Balance => "Failed to get balance",
    };

    (e as i64, msg.to_string())
}

pub fn server_error(e: RpcError, id: Value) -> JsonResult {
    let (code, msg) = to_tuple(e);
    JsonError::new(ServerError(code), Some(msg), id).into()
}
