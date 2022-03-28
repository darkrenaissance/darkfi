use std::error::Error;

use futures::prelude::*;
use url::Url;

pub mod tcp;
pub use tcp::TcpTransport;

pub trait Transport {
    type Output;
    type Error: Error;
    type Dial: Future<Output = Result<Self::Output, Self::Error>>;

    fn dial(self, url: Url) -> Result<Self::Dial, TransportError<Self::Error>>
    where
        Self: Sized;
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError<TErr> {
    #[error("Address not supported: {0}")]
    AddrNotSupported(Url),

    #[error("Transport IO Error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("{0}")]
    Other(TErr),
}
