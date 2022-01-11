pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    /// Service
    #[error("Services Error: `{0}`")]
    ServicesError(&'static str),
    #[error("Client failed: `{0}`")]
    ClientFailed(String),
    #[cfg(feature = "btc")]
    #[error(transparent)]
    BtcFailed(#[from] crate::service::BtcFailed),
    #[cfg(feature = "sol")]
    #[error("Sol client failed: `{0}`")]
    SolFailed(String),
    #[cfg(feature = "eth")]
    #[error(transparent)]
    EthFailed(#[from] crate::service::EthFailed),
    #[error("BridgeError Error: `{0}`")]
    BridgeError(String),

    #[error("Async_channel sender error")]
    AsyncChannelSenderError,
    #[error(transparent)]
    AsyncChannelReceiverError(#[from] async_channel::RecvError),
}

#[cfg(feature = "sol")]
impl From<crate::service::SolFailed> for Error {
    fn from(err: crate::service::SolFailed) -> Error {
        Error::SolFailed(err.to_string())
    }
}

impl From<darkfi::Error> for Error {
    fn from(err: darkfi::Error) -> Error {
        Error::ClientFailed(err.to_string())
    }
}

impl<T> From<async_channel::SendError<T>> for Error {
    fn from(_err: async_channel::SendError<T>) -> Error {
        Error::AsyncChannelSenderError
    }
}
