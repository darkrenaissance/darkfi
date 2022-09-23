//use serde_json::Value;
//use darkfi::rpc::jsonrpc::{error as jsonerr, response as jsonresp, ErrorCode, JsonResult};

#[derive(Debug, thiserror::Error)]
pub enum DnetViewError {
    #[error("RPC reply is empty")]
    EmptyRpcReply,
    #[error("Json Value is not an object")]
    ValueIsNotObject,
    #[error("Failed to find ID at current index")]
    NoIdAtIndex,
    #[error("Message log does not contain ID")]
    CannotFindId,
    #[error("ID does not return a selectable object")]
    NotSelectableObject,
    #[error("JSON data does not contain an external addr")]
    NoExternalAddr,
    #[error("Found unexpected data in View")]
    UnexpectedData(String),
    #[error("InternalError")]
    Darkfi(#[from] darkfi::error::Error),
    #[error("Json serialization error: `{0}`")]
    SerdeJsonError(String),
    #[error("IO error: {0}")]
    Io(std::io::ErrorKind),
    #[error("SetLogger (log crate) failed: {0}")]
    SetLoggerError(String),
    #[error("URL parse error: {0}")]
    UrlParse(String),
}

pub type DnetViewResult<T> = std::result::Result<T, DnetViewError>;

impl From<serde_json::Error> for DnetViewError {
    fn from(err: serde_json::Error) -> DnetViewError {
        DnetViewError::SerdeJsonError(err.to_string())
    }
}

impl From<std::io::Error> for DnetViewError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.kind())
    }
}

impl From<log::SetLoggerError> for DnetViewError {
    fn from(err: log::SetLoggerError) -> Self {
        Self::SetLoggerError(err.to_string())
    }
}

impl From<url::ParseError> for DnetViewError {
    fn from(err: url::ParseError) -> Self {
        Self::UrlParse(err.to_string())
    }
}
