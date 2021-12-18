use crate::error::Error as DarkFiError;
use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    SqliteError(String),
    DarkFiError(DarkFiError),
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match *self {
            Error::SqliteError(ref e) => write!(f, "database sqlite error: {}", e),
            Error::DarkFiError(ref e) => write!(f, "darkfi internal error : {}", e),
        }
    }
}

impl From<DarkFiError> for Error {
    fn from(err: DarkFiError) -> Error {
        Error::DarkFiError(err)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Error {
        Error::SqliteError(err.to_string())
    }
}

