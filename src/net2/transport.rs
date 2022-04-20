use async_std::sync::Arc;
use std::error::Error;

use async_trait::async_trait;
use futures::prelude::*;
use url::Url;

mod tcp;
mod tls;
mod tor;

pub use tcp::TcpTransport;
pub use tls::TlsTransport;
pub use tor::TorTransport;

#[async_trait]
pub trait Transport: Sync + Send + 'static + Clone {
    type Acceptor: Sync + Send;
    type Connector: Sync + Send + AsyncRead + AsyncWrite;

    type Error: Error;

    type Listener: Future<Output = Result<Self::Acceptor, Self::Error>> + Sync + Send;
    type Dial: Future<Output = Result<Self::Connector, Self::Error>> + Sync + Send;

    fn listen_on(self, url: Url) -> Result<Self::Listener, TransportError<Self::Error>>
    where
        Self: Sized;

    fn dial(self, url: Url) -> Result<Self::Dial, TransportError<Self::Error>>
    where
        Self: Sized;

    fn new(ttl: Option<u32>, backlog: i32) -> Self;

    async fn accept(listener: Arc<Self::Acceptor>) -> Self::Connector;
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
unsafe impl<TErr> Sync for TransportError<TErr> {}
unsafe impl<TErr> Send for TransportError<TErr> {}
