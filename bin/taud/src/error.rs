#[derive(Debug, thiserror::Error)]
pub enum TaudError {
    #[error("Due timestamp invalid")]
    InvalidDueTime,
    #[error("Invalid Id")]
    InvalidId,
    #[error("Invalid Data: `{0}` ")]
    InvalidData(String),
    #[error(transparent)]
    Darkfi(#[from] darkfi::error::Error),
}

pub type TaudResult<T> = std::result::Result<T, TaudError>;
