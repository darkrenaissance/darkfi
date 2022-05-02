use async_trait::async_trait;
use futures::prelude::*;
use futures_rustls::{TlsAcceptor, TlsStream};
use url::Url;

use crate::Result;

mod upgrade_tls;
pub use upgrade_tls::TlsUpgrade;

mod tcp;
pub use tcp::TcpTransport;

mod tor;
pub use tor::TorTransport;

/// The `Transport` trait serves as a base for implementing transport protocols.
/// Base transports can optionally be upgraded with TLS in order to support encryption.
/// The implementation of our TLS authentication can be found in the [`upgrade_tls`] module.
#[async_trait]
pub trait Transport {
    type Acceptor;
    type Connector;

    type Listener: Future<Output = Result<Self::Acceptor>>;
    type Dial: Future<Output = Result<Self::Connector>>;

    type TlsListener: Future<Output = Result<(TlsAcceptor, Self::Acceptor)>>;
    type TlsDialer: Future<Output = Result<TlsStream<Self::Connector>>>;

    fn listen_on(self, url: Url) -> Result<Self::Listener>
    where
        Self: Sized;

    fn upgrade_listener(self, acceptor: Self::Acceptor) -> Result<Self::TlsListener>
    where
        Self: Sized;

    fn dial(self, url: Url) -> Result<Self::Dial>
    where
        Self: Sized;

    fn upgrade_dialer(self, stream: Self::Connector) -> Result<Self::TlsDialer>
    where
        Self: Sized;
}
