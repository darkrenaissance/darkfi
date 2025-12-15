/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::sync::Arc;

use darkfi::{
    error::RpcError,
    rpc::jsonrpc::{ErrorCode, JsonError},
    Error,
};

// Constant for the error code
pub const ERROR_CODE_PING_DARKFID_FAILED: i32 = -32300;

/// Custom RPC errors available for blockchain explorer.
/// These represent specific RPC-related failures.
#[derive(Debug, thiserror::Error)]
pub enum ExplorerdError {
    #[error("Ping darkfid failed: {0}")]
    PingDarkfidFailed(String),

    #[error("Invalid contract ID: {0}")]
    InvalidContractId(String),

    #[error("Invalid header hash: {0}")]
    InvalidHeaderHash(String),

    #[error("Invalid tx hash: {0}")]
    InvalidTxHash(String),
}

/// Provides a conversion from [`ExplorerdError`] to darkfi [`Error`] type.
impl From<ExplorerdError> for Error {
    fn from(err: ExplorerdError) -> Self {
        let error: RpcError = err.into();
        error.into()
    }
}

/// Conversion from [`ExplorerdRpcError`] to [`RpcError`]
impl From<ExplorerdError> for RpcError {
    fn from(err: ExplorerdError) -> Self {
        RpcError::ServerError(Arc::new(err))
    }
}

/// Helper function to convert `ExplorerdRpcError` into error code with corresponding error message.
pub fn to_error_code_message(e: &ExplorerdError) -> (i32, String) {
    match e {
        ExplorerdError::PingDarkfidFailed(_) => (ERROR_CODE_PING_DARKFID_FAILED, e.to_string()),
        ExplorerdError::InvalidContractId(_) |
        ExplorerdError::InvalidHeaderHash(_) |
        ExplorerdError::InvalidTxHash(_) => (ErrorCode::InvalidParams.code(), e.to_string()),
    }
}

/// Constructs a [`JsonError`] representing a server error using the provided
/// [`ExplorerdError`] , request ID, and optional custom message, returning a [`JsonError`]
/// with a corresponding server error code and message.
pub fn server_error(e: &ExplorerdError, id: u16, msg: Option<&str>) -> JsonError {
    let (code, default_msg) = to_error_code_message(e);

    // Use the provided custom message if available; otherwise, use the default.
    let message = msg.unwrap_or(&default_msg).to_string();

    JsonError::new(ErrorCode::ServerError(code), Some(message), id)
}
