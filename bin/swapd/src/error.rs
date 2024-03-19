use crate::{ethereum, protocol};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("protocol error: {0}")]
    ProtocolError(#[source] protocol::Error),
    #[error("ethereum error: {0}")]
    EthereumError(#[source] ethereum::Error),
}

impl From<protocol::Error> for Error {
    fn from(e: protocol::Error) -> Self {
        Error::ProtocolError(e)
    }
}

impl From<ethereum::Error> for Error {
    fn from(e: ethereum::Error) -> Self {
        Error::EthereumError(e)
    }
}
