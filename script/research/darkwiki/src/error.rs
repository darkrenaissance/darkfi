#[derive(thiserror::Error, Debug)]
pub enum DarkWikiError {
    #[error("Add Operation failed")]
    AddOperationFailed,
    #[error("Encryption error: `{0}`")]
    EncryptionError(String),
    #[error("Json serialization error: `{0}`")]
    SerdeJsonError(String),
    #[error("InternalError")]
    Darkfi(#[from] darkfi::error::Error),
}

pub type DarkWikiResult<T> = std::result::Result<T, DarkWikiError>;

impl From<serde_json::Error> for DarkWikiError {
    fn from(err: serde_json::Error) -> DarkWikiError {
        DarkWikiError::SerdeJsonError(err.to_string())
    }
}

impl From<crypto_box::aead::Error> for DarkWikiError {
    fn from(err: crypto_box::aead::Error) -> DarkWikiError {
        DarkWikiError::EncryptionError(err.to_string())
    }
}
