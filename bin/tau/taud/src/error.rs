#[derive(Debug, thiserror::Error)]
pub enum TaudError {
    #[error("Due timestamp invalid")]
    InvalidDueTime,
    #[error("Invalid Id")]
    InvalidId,
    #[error("Invalid Data/Params: `{0}` ")]
    InvalidData(String),
    #[error(transparent)]
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
