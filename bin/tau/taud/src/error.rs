/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use darkfi::rpc::jsonrpc::{ErrorCode, JsonError, JsonResponse, JsonResult};
use tinyjson::JsonValue;

#[derive(Debug, thiserror::Error)]
pub enum TaudError {
    #[error("Due timestamp invalid")]
    InvalidDueTime,
    #[error("Invalid Id")]
    InvalidId,
    #[error("Invalid Data/Params: `{0}` ")]
    InvalidData(String),
    #[error("InternalError")]
    Darkfi(#[from] darkfi::error::Error),
    #[error("Json serialization error: `{0}`")]
    JsonError(String),
    #[error("Encryption error: `{0}`")]
    EncryptionError(String),
    #[error("Decryption error: `{0}`")]
    DecryptionError(String),
    #[error("IO Error: `{0}`")]
    IoError(String),
}

pub type TaudResult<T> = std::result::Result<T, TaudError>;

impl From<crypto_box::aead::Error> for TaudError {
    fn from(err: crypto_box::aead::Error) -> TaudError {
        TaudError::EncryptionError(err.to_string())
    }
}

impl From<std::io::Error> for TaudError {
    fn from(err: std::io::Error) -> TaudError {
        TaudError::IoError(err.to_string())
    }
}

pub fn to_json_result(res: TaudResult<JsonValue>, id: u16) -> JsonResult {
    match res {
        Ok(v) => JsonResponse::new(v, id).into(),
        Err(err) => match err {
            TaudError::InvalidId => {
                JsonError::new(ErrorCode::InvalidParams, Some("invalid task id".into()), id).into()
            }
            TaudError::InvalidData(e) | TaudError::JsonError(e) => {
                JsonError::new(ErrorCode::InvalidParams, Some(e), id).into()
            }
            TaudError::InvalidDueTime => {
                JsonError::new(ErrorCode::InvalidParams, Some("invalid due time".into()), id).into()
            }
            TaudError::EncryptionError(e) => {
                JsonError::new(ErrorCode::InternalError, Some(e), id).into()
            }
            TaudError::DecryptionError(e) => {
                JsonError::new(ErrorCode::InternalError, Some(e), id).into()
            }
            TaudError::Darkfi(e) => {
                JsonError::new(ErrorCode::InternalError, Some(e.to_string()), id).into()
            }
            TaudError::IoError(e) => JsonError::new(ErrorCode::InternalError, Some(e), id).into(),
        },
    }
}
