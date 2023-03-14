/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{net::SocketAddr, time::Duration};

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

mod unix;
pub use unix::UnixTransport;

mod nym;
pub use nym::NymTransport;

/// A helper function to convert SocketAddr to Url and add scheme
pub(crate) fn socket_addr_to_url(addr: SocketAddr, scheme: &str) -> Result<Url> {
    let url = Url::parse(&format!("{}://{}", scheme, addr))?;
    Ok(url)
}

/// Used as wrapper for stream used by Transport trait
pub trait TransportStream: AsyncWrite + AsyncRead + Unpin + Send + Sync {}

/// Used as wrapper for listener used by Transport trait
#[async_trait]
pub trait TransportListener: Send + Sync + Unpin {
    async fn next(&self) -> Result<(Box<dyn TransportStream>, Url)>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportName {
    Tcp(Option<String>),
    Tor(Option<String>),
    Nym(Option<String>),
    Unix,
}

impl TransportName {
    pub fn to_scheme(&self) -> String {
        match self {
            Self::Tcp(None) => "tcp".into(),
            Self::Tcp(Some(opt)) => format!("tcp+{}", opt),
            Self::Tor(None) => "tor".into(),
            Self::Tor(Some(opt)) => format!("tor+{}", opt),
            Self::Nym(None) => "nym".into(),
            Self::Nym(Some(opt)) => format!("nym+{}", opt),
            Self::Unix => "unix".into(),
        }
    }
}

impl TryFrom<&str> for TransportName {
    type Error = crate::Error;

    fn try_from(scheme: &str) -> Result<Self> {
        let transport_name = match scheme {
            "tcp" => Self::Tcp(None),
            "tcp+tls" | "tls" => Self::Tcp(Some("tls".into())),
            "tor" => Self::Tor(None),
            "tor+tls" => Self::Tor(Some("tls".into())),
            "nym" => Self::Nym(None),
            "nym+tls" => Self::Nym(Some("tls".into())),
            "unix" => Self::Unix,
            n => return Err(crate::Error::UnsupportedTransport(n.into())),
        };
        Ok(transport_name)
    }
}

impl TryFrom<Url> for TransportName {
    type Error = crate::Error;

    fn try_from(url: Url) -> Result<Self> {
        Self::try_from(url.scheme())
    }
}

/// The `Transport` trait serves as a base for implementing transport protocols.
/// Base transports can optionally be upgraded with TLS in order to support encryption.
/// The implementation of our TLS authentication can be found in the
/// [`upgrade_tls`](TlsUpgrade) module.
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

    fn dial(self, url: Url, timeout: Option<Duration>) -> Result<Self::Dial>
    where
        Self: Sized;

    fn upgrade_dialer(self, stream: Self::Connector) -> Result<Self::TlsDialer>
    where
        Self: Sized;
}
