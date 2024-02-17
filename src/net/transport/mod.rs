/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use log::warn;
use std::time::Duration;
use tor_error::ErrorReport;

use async_trait::async_trait;
use smol::io::{AsyncRead, AsyncWrite};
use url::Url;

use crate::{Error, Result};

/// TLS upgrade mechanism
pub(crate) mod tls;

#[cfg(feature = "p2p-tcp")]
/// TCP transport
pub(crate) mod tcp;

#[cfg(feature = "p2p-tor")]
/// Tor transport
pub(crate) mod tor;

#[cfg(feature = "p2p-nym")]
/// Nym transport
pub(crate) mod nym;

#[cfg(feature = "p2p-unix")]
/// Unix socket transport
pub(crate) mod unix;

/// Dialer variants
#[derive(Debug, Clone)]
pub enum DialerVariant {
    #[cfg(feature = "p2p-tcp")]
    /// Plain TCP
    Tcp(tcp::TcpDialer),

    #[cfg(feature = "p2p-tcp")]
    /// TCP with TLS
    TcpTls(tcp::TcpDialer),

    #[cfg(feature = "p2p-tor")]
    /// Tor
    Tor(tor::TorDialer),

    #[cfg(feature = "p2p-tor")]
    /// Tor with TLS
    TorTls(tor::TorDialer),

    #[cfg(feature = "p2p-nym")]
    /// Nym
    Nym(nym::NymDialer),

    #[cfg(feature = "p2p-nym")]
    /// Nym with TLS
    NymTls(nym::NymDialer),

    #[cfg(feature = "p2p-unix")]
    /// Unix socket
    Unix(unix::UnixDialer),
}

/// Listener variants
#[derive(Debug, Clone)]
pub enum ListenerVariant {
    #[cfg(feature = "p2p-tcp")]
    /// Plain TCP
    Tcp(tcp::TcpListener),

    #[cfg(feature = "p2p-tcp")]
    /// TCP with TLS
    TcpTls(tcp::TcpListener),

    #[cfg(feature = "p2p-unix")]
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
            #[cfg(feature = "p2p-tcp")]
            "tcp" => {
                // Build a TCP dialer
                enforce_hostport!(endpoint);
                let variant = tcp::TcpDialer::new(None).await?;
                let variant = DialerVariant::Tcp(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-tcp")]
            "tcp+tls" => {
                // Build a TCP dialer wrapped with TLS
                enforce_hostport!(endpoint);
                let variant = tcp::TcpDialer::new(None).await?;
                let variant = DialerVariant::TcpTls(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-tor")]
            "tor" => {
                // Build a Tor dialer
                enforce_hostport!(endpoint);
                let variant = tor::TorDialer::new().await?;
                let variant = DialerVariant::Tor(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-tor")]
            "tor+tls" => {
                // Build a Tor dialer wrapped with TLS
                enforce_hostport!(endpoint);
                let variant = tor::TorDialer::new().await?;
                let variant = DialerVariant::TorTls(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-nym")]
            "nym" => {
                // Build a Nym dialer
                enforce_hostport!(endpoint);
                let variant = nym::NymDialer::new().await?;
                let variant = DialerVariant::Nym(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-nym")]
            "nym+tls" => {
                // Build a Nym dialer wrapped with TLS
                enforce_hostport!(endpoint);
                let variant = nym::NymDialer::new().await?;
                let variant = DialerVariant::NymTls(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-unix")]
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

    /// The Tor-based Dialer variants can panic: this is intended. There exists validation
    /// for hosts and ports in other parts of the codebase. A panic occurring here
    /// likely indicates a configuration issue on the part of the user. It is preferable
    /// in this case that the user is alerted to this problem via a panic.
    pub async fn dial(&self, timeout: Option<Duration>) -> Result<Box<dyn PtStream>> {
        match &self.variant {
            #[cfg(feature = "p2p-tcp")]
            DialerVariant::Tcp(dialer) => {
                // NOTE: sockaddr here is an array, can contain both ipv4 and ipv6
                let sockaddr = self.endpoint.socket_addrs(|| None)?;
                let stream = dialer.do_dial(sockaddr[0], timeout).await?;
                Ok(Box::new(stream))
            }

            #[cfg(feature = "p2p-tcp")]
            DialerVariant::TcpTls(dialer) => {
                let sockaddr = self.endpoint.socket_addrs(|| None)?;
                let stream = dialer.do_dial(sockaddr[0], timeout).await?;
                let tlsupgrade = tls::TlsUpgrade::new().await;
                let stream = tlsupgrade.upgrade_dialer_tls(stream).await?;
                Ok(Box::new(stream))
            }

            #[cfg(feature = "p2p-tor")]
            DialerVariant::Tor(dialer) => {
                let host = self.endpoint.host_str().unwrap();
                let port = self.endpoint.port().unwrap();
                // Extract error reports (i.e. very detailed debugging)
                // from arti-client in order to help debug Tor connections.
                // https://docs.rs/arti-client/latest/arti_client/#reporting-arti-errors
                // https://gitlab.torproject.org/tpo/core/arti/-/issues/1086
                let result = match dialer.do_dial(host, port, timeout).await {
                    Ok(stream) => Ok(stream),
                    Err(err) => {
                        warn!("{}", err.report());
                        Err(err)
                    }
                };
                let stream = result?;
                Ok(Box::new(stream))
            }

            #[cfg(feature = "p2p-tor")]
            DialerVariant::TorTls(dialer) => {
                let host = self.endpoint.host_str().unwrap();
                let port = self.endpoint.port().unwrap();
                // Extract error reports (i.e. very detailed debugging)
                // from arti-client in order to help debug Tor connections.
                // https://docs.rs/arti-client/latest/arti_client/#reporting-arti-errors
                // https://gitlab.torproject.org/tpo/core/arti/-/issues/1086
                let result = match dialer.do_dial(host, port, timeout).await {
                    Ok(stream) => Ok(stream),
                    Err(err) => {
                        warn!("{}", err.report());
                        Err(err)
                    }
                };
                let stream = result?;
                let tlsupgrade = tls::TlsUpgrade::new().await;
                let stream = tlsupgrade.upgrade_dialer_tls(stream).await?;
                Ok(Box::new(stream))
            }

            #[cfg(feature = "p2p-nym")]
            DialerVariant::Nym(_dialer) => {
                todo!();
            }

            #[cfg(feature = "p2p-nym")]
            DialerVariant::NymTls(_dialer) => {
                todo!();
            }

            #[cfg(feature = "p2p-unix")]
            DialerVariant::Unix(dialer) => {
                let path = self.endpoint.to_file_path()?;
                let stream = dialer.do_dial(path).await?;
                Ok(Box::new(stream))
            }

            #[cfg(not(any(
                feature = "p2p-tcp",
                feature = "p2p-tor",
                feature = "p2p-nym",
                feature = "p2p-unix"
            )))]
            _ => panic!("No compiled p2p transports!"),
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
            #[cfg(feature = "p2p-tcp")]
            "tcp" => {
                // Build a TCP listener
                enforce_hostport!(endpoint);
                let variant = tcp::TcpListener::new(1024).await?;
                let variant = ListenerVariant::Tcp(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-tcp")]
            "tcp+tls" => {
                // Build a TCP listener wrapped with TLS
                enforce_hostport!(endpoint);
                let variant = tcp::TcpListener::new(1024).await?;
                let variant = ListenerVariant::TcpTls(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-unix")]
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
            #[cfg(feature = "p2p-tcp")]
            ListenerVariant::Tcp(listener) => {
                let sockaddr = self.endpoint.socket_addrs(|| None)?;
                let l = listener.do_listen(sockaddr[0]).await?;
                Ok(Box::new(l))
            }

            #[cfg(feature = "p2p-tcp")]
            ListenerVariant::TcpTls(listener) => {
                let sockaddr = self.endpoint.socket_addrs(|| None)?;
                let l = listener.do_listen(sockaddr[0]).await?;
                let tlsupgrade = tls::TlsUpgrade::new().await;
                let l = tlsupgrade.upgrade_listener_tcp_tls(l).await?;
                Ok(Box::new(l))
            }

            #[cfg(feature = "p2p-unix")]
            ListenerVariant::Unix(listener) => {
                let path = self.endpoint.to_file_path()?;
                let l = listener.do_listen(&path).await?;
                Ok(Box::new(l))
            }

            #[cfg(not(any(feature = "p2p-tcp", feature = "p2p-unix")))]
            _ => panic!("No compiled p2p transports!"),
        }
    }

    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }
}

/// Wrapper trait for async streams
pub trait PtStream: AsyncRead + AsyncWrite + Unpin + Send {}

#[cfg(feature = "p2p-tcp")]
impl PtStream for smol::net::TcpStream {}

#[cfg(feature = "p2p-tcp")]
impl PtStream for futures_rustls::TlsStream<smol::net::TcpStream> {}

#[cfg(feature = "p2p-tor")]
impl PtStream for arti_client::DataStream {}

#[cfg(feature = "p2p-tor")]
impl PtStream for futures_rustls::TlsStream<arti_client::DataStream> {}

#[cfg(feature = "p2p-unix")]
impl PtStream for unix::UnixStream {}

/// Wrapper trait for async listeners
#[async_trait]
pub trait PtListener: Send + Sync + Unpin {
    async fn next(&self) -> std::io::Result<(Box<dyn PtStream>, Url)>;
}
