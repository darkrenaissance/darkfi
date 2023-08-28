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

use std::time::Duration;

use async_trait::async_trait;
use smol::io::{AsyncRead, AsyncWrite};
use url::Url;

use crate::{Error, Result};

/// TLS Upgrade Mechanism
pub(crate) mod tls;

#[cfg(feature = "p2p-transport-tcp")]
/// TCP Transport
pub(crate) mod tcp;

#[cfg(feature = "p2p-transport-tor")]
/// Tor transport
pub(crate) mod tor;

#[cfg(feature = "p2p-transport-nym")]
/// Nym transport
pub(crate) mod nym;

#[cfg(feature = "p2p-transport-unix")]
/// Unix socket transport
pub(crate) mod unix;

/// Dialer variants
#[derive(Debug, Clone)]
pub enum DialerVariant {
    #[cfg(feature = "p2p-transport-tcp")]
    /// Plain TCP
    Tcp(tcp::TcpDialer),

    #[cfg(feature = "p2p-transport-tcp")]
    /// TCP with TLS
    TcpTls(tcp::TcpDialer),

    #[cfg(feature = "p2p-transport-tor")]
    /// Tor
    Tor(tor::TorDialer),

    #[cfg(feature = "p2p-transport-tor")]
    /// Tor with TLS
    TorTls(tor::TorDialer),

    #[cfg(feature = "p2p-transport-nym")]
    /// Nym
    Nym(nym::NymDialer),

    #[cfg(feature = "p2p-transport-nym")]
    /// Nym with TLS
    NymTls(nym::NymDialer),

    #[cfg(feature = "p2p-transport-unix")]
    /// Unix socket
    Unix(unix::UnixDialer),
}

/// Listener variants
#[derive(Debug, Clone)]
pub enum ListenerVariant {
    #[cfg(feature = "p2p-transport-tcp")]
    /// Plain TCP
    Tcp(tcp::TcpListener),

    #[cfg(feature = "p2p-transport-tcp")]
    /// TCP with TLS
    TcpTls(tcp::TcpListener),

    #[cfg(feature = "p2p-transport-unix")]
    /// Unix socket
    Unix(unix::UnixListener),
}

/// A dialer that is able to transparently operate over arbitrary transports.
pub struct Dialer {
    /// The endpoint to connect to
    endpoint: Url,
    /// The dialer variant (transport protocol)
    variant: DialerVariant,
}

macro_rules! enforce_hostport {
    ($endpoint:ident) => {
        if $endpoint.host_str().is_none() || $endpoint.port().is_none() {
            return Err(Error::InvalidDialerScheme)
        }
    };
}

macro_rules! enforce_abspath {
    ($endpoint:ident) => {
        if $endpoint.host_str().is_some() || $endpoint.port().is_some() {
            return Err(Error::InvalidDialerScheme)
        }

        if $endpoint.to_file_path().is_err() {
            return Err(Error::InvalidDialerScheme)
        }
    };
}

impl Dialer {
    /// Instantiate a new [`Dialer`] with the given [`Url`].
    pub async fn new(endpoint: Url) -> Result<Self> {
        match endpoint.scheme().to_lowercase().as_str() {
            #[cfg(feature = "p2p-transport-tcp")]
            "tcp" => {
                // Build a TCP dialer
                enforce_hostport!(endpoint);
                let variant = tcp::TcpDialer::new(None).await?;
                let variant = DialerVariant::Tcp(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-transport-tcp")]
            "tcp+tls" => {
                // Build a TCP dialer wrapped with TLS
                enforce_hostport!(endpoint);
                let variant = tcp::TcpDialer::new(None).await?;
                let variant = DialerVariant::TcpTls(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-transport-tor")]
            "tor" => {
                // Build a Tor dialer
                enforce_hostport!(endpoint);
                let variant = tor::TorDialer::new().await?;
                let variant = DialerVariant::Tor(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-transport-tor")]
            "tor+tls" => {
                // Build a Tor dialer wrapped with TLS
                enforce_hostport!(endpoint);
                let variant = tor::TorDialer::new().await?;
                let variant = DialerVariant::TorTls(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-transport-nym")]
            "nym" => {
                // Build a Nym dialer
                enforce_hostport!(endpoint);
                let variant = nym::NymDialer::new().await?;
                let variant = DialerVariant::Nym(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-transport-nym")]
            "nym+tls" => {
                // Build a Nym dialer wrapped with TLS
                enforce_hostport!(endpoint);
                let variant = nym::NymDialer::new().await?;
                let variant = DialerVariant::NymTls(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-transport-unix")]
            "unix" => {
                enforce_abspath!(endpoint);
                // Build a Unix socket dialer
                let variant = unix::UnixDialer::new().await?;
                let variant = DialerVariant::Unix(variant);
                Ok(Self { endpoint, variant })
            }

            x => Err(Error::UnsupportedTransport(x.to_string())),
        }
    }

    /// Dial an instantiated [`Dialer`]. This creates a connection and returns a stream.
    pub async fn dial(&self, timeout: Option<Duration>) -> Result<Box<dyn PtStream>> {
        match &self.variant {
            #[cfg(feature = "p2p-transport-tcp")]
            DialerVariant::Tcp(dialer) => {
                // NOTE: sockaddr here is an array, can contain both ipv4 and ipv6
                let sockaddr = self.endpoint.socket_addrs(|| None)?;
                let stream = dialer.do_dial(sockaddr[0], timeout).await?;
                Ok(Box::new(stream))
            }

            #[cfg(feature = "p2p-transport-tcp")]
            DialerVariant::TcpTls(dialer) => {
                let sockaddr = self.endpoint.socket_addrs(|| None)?;
                let stream = dialer.do_dial(sockaddr[0], timeout).await?;
                let tlsupgrade = tls::TlsUpgrade::new();
                let stream = tlsupgrade.upgrade_dialer_tls(stream).await?;
                Ok(Box::new(stream))
            }

            #[cfg(feature = "p2p-transport-tor")]
            DialerVariant::Tor(dialer) => {
                let host = self.endpoint.host_str().unwrap();
                let port = self.endpoint.port().unwrap();
                let stream = dialer.do_dial(host, port, timeout).await?;
                Ok(Box::new(stream))
            }

            #[cfg(feature = "p2p-transport-tor")]
            DialerVariant::TorTls(dialer) => {
                let host = self.endpoint.host_str().unwrap();
                let port = self.endpoint.port().unwrap();
                let stream = dialer.do_dial(host, port, timeout).await?;
                let tlsupgrade = tls::TlsUpgrade::new();
                let stream = tlsupgrade.upgrade_dialer_tls(stream).await?;
                Ok(Box::new(stream))
            }

            #[cfg(feature = "p2p-transport-nym")]
            DialerVariant::Nym(_dialer) => {
                todo!();
            }

            #[cfg(feature = "p2p-transport-nym")]
            DialerVariant::NymTls(_dialer) => {
                todo!();
            }

            #[cfg(feature = "p2p-transport-unix")]
            DialerVariant::Unix(dialer) => {
                let path = self.endpoint.to_file_path()?;
                let stream = dialer.do_dial(path).await?;
                Ok(Box::new(stream))
            }
        }
    }

    /// Return a reference to the `Dialer` endpoint
    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }
}

/// A listener that is able to transparently listen over arbitrary transports.
pub struct Listener {
    /// The address to open the listener on
    endpoint: Url,
    /// The listener variant (transport protocol)
    variant: ListenerVariant,
}

impl Listener {
    /// Instantiate a new [`Listener`] with the given [`Url`].
    /// Must contain a scheme, host string, and a port.
    pub async fn new(endpoint: Url) -> Result<Self> {
        match endpoint.scheme().to_lowercase().as_str() {
            #[cfg(feature = "p2p-transport-tcp")]
            "tcp" => {
                // Build a TCP listener
                enforce_hostport!(endpoint);
                let variant = tcp::TcpListener::new(1024).await?;
                let variant = ListenerVariant::Tcp(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-transport-tcp")]
            "tcp+tls" => {
                // Build a TCP listener wrapped with TLS
                enforce_hostport!(endpoint);
                let variant = tcp::TcpListener::new(1024).await?;
                let variant = ListenerVariant::TcpTls(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-transport-unix")]
            "unix" => {
                enforce_abspath!(endpoint);
                let variant = unix::UnixListener::new().await?;
                let variant = ListenerVariant::Unix(variant);
                Ok(Self { endpoint, variant })
            }

            x => Err(Error::UnsupportedTransport(x.to_string())),
        }
    }

    /// Listen on an instantiated [`Listener`].
    /// This will open a socket and return the listener.
    pub async fn listen(&self) -> Result<Box<dyn PtListener>> {
        match &self.variant {
            #[cfg(feature = "p2p-transport-tcp")]
            ListenerVariant::Tcp(listener) => {
                let sockaddr = self.endpoint.socket_addrs(|| None)?;
                let l = listener.do_listen(sockaddr[0]).await?;
                Ok(Box::new(l))
            }

            #[cfg(feature = "p2p-transport-tcp")]
            ListenerVariant::TcpTls(listener) => {
                let sockaddr = self.endpoint.socket_addrs(|| None)?;
                let l = listener.do_listen(sockaddr[0]).await?;
                let tlsupgrade = tls::TlsUpgrade::new();
                let l = tlsupgrade.upgrade_listener_tcp_tls(l).await?;
                Ok(Box::new(l))
            }

            #[cfg(feature = "p2p-transport-unix")]
            ListenerVariant::Unix(listener) => {
                let path = self.endpoint.to_file_path()?;
                let l = listener.do_listen(&path).await?;
                Ok(Box::new(l))
            }
        }
    }

    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }
}

/// Wrapper trait for async streams
pub trait PtStream: AsyncRead + AsyncWrite + Unpin + Send {}

#[cfg(feature = "p2p-transport-tcp")]
impl PtStream for smol::net::TcpStream {}

#[cfg(feature = "p2p-transport-tcp")]
impl PtStream for async_rustls::TlsStream<smol::net::TcpStream> {}

#[cfg(feature = "p2p-transport-tor")]
impl PtStream for arti_client::DataStream {}

#[cfg(feature = "p2p-transport-tor")]
impl PtStream for async_rustls::TlsStream<arti_client::DataStream> {}

#[cfg(feature = "p2p-transport-unix")]
impl PtStream for smol::net::unix::UnixStream {}

/// Wrapper trait for async listeners
#[async_trait]
pub trait PtListener: Send + Sync + Unpin {
    async fn next(&self) -> std::io::Result<(Box<dyn PtStream>, Url)>;
}
