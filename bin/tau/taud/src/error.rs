use serde_json::Value;

use darkfi::rpc::jsonrpc::{error as jsonerr, response as jsonresp, ErrorCode, JsonResult};

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
    SerdeJsonError(String),
}

pub type TaudResult<T> = std::result::Result<T, TaudError>;

impl From<serde_json::Error> for TaudError {
    fn from(err: serde_json::Error) -> TaudError {
        TaudError::SerdeJsonError(err.to_string())
    }
}

pub fn to_json_result(res: TaudResult<Value>, id: Value) -> JsonResult {
    match res {
        Ok(v) => JsonResult::Resp(jsonresp(v, id)),
        Err(err) => match err {
            TaudError::InvalidId => JsonResult::Err(jsonerr(
                ErrorCode::InvalidParams,
                Some("invalid task's id".into()),
                id,
            )),
            TaudError::InvalidData(e) | TaudError::SerdeJsonError(e) => {
                JsonResult::Err(jsonerr(ErrorCode::InvalidParams, Some(e), id))
            }
            TaudError::InvalidDueTime => JsonResult::Err(jsonerr(
                ErrorCode::InvalidParams,
                Some("invalid due time".into()),
                id,
            )),
            TaudError::Darkfi(e) => {
                JsonResult::Err(jsonerr(ErrorCode::InternalError, Some(e.to_string()), id))
            }
        },
    }
}
