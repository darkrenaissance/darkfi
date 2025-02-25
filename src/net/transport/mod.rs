/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::{io, time::Duration};

use async_trait::async_trait;
use log::error;
use smol::io::{AsyncRead, AsyncWrite};
use url::Url;

#[cfg(feature = "p2p-unix")]
use std::io::ErrorKind;

/// TLS upgrade mechanism
pub(crate) mod tls;

/// SOCKS5 proxy client
pub mod socks5;

/// TCP transport
pub(crate) mod tcp;

#[cfg(feature = "p2p-tor")]
/// Tor transport
pub(crate) mod tor;

#[cfg(feature = "p2p-nym")]
/// Nym transport
pub(crate) mod nym;

/// Unix socket transport
#[cfg(feature = "p2p-unix")]
pub(crate) mod unix;

/// Dialer variants
#[derive(Debug, Clone)]
pub enum DialerVariant {
    /// Plain TCP
    Tcp(tcp::TcpDialer),

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

    /// Unix socket
    #[cfg(feature = "p2p-unix")]
    Unix(unix::UnixDialer),

    /// SOCKS5 proxy
    Socks5(socks5::Socks5Dialer),

    /// SOCKS5 proxy with TLS
    Socks5Tls(socks5::Socks5Dialer),
}

/// Listener variants
#[derive(Debug, Clone)]
pub enum ListenerVariant {
    /// Plain TCP
    Tcp(tcp::TcpListener),

    /// TCP with TLS
    TcpTls(tcp::TcpListener),

    #[cfg(feature = "p2p-tor")]
    /// Tor
    Tor(tor::TorListener),

    /// Unix socket
    #[cfg(feature = "p2p-unix")]
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
            return Err(io::Error::from_raw_os_error(libc::ENETUNREACH))
        }
    };
}

#[cfg(feature = "p2p-unix")]
macro_rules! enforce_abspath {
    ($endpoint:ident) => {
        if $endpoint.host_str().is_some() || $endpoint.port().is_some() {
            return Err(io::Error::from_raw_os_error(libc::ENETUNREACH))
        }

        if $endpoint.to_file_path().is_err() {
            return Err(io::Error::from_raw_os_error(libc::ENETUNREACH))
        }
    };
}

impl Dialer {
    /// Instantiate a new [`Dialer`] with the given [`Url`] and datastore path.
    pub async fn new(endpoint: Url, datastore: Option<String>) -> io::Result<Self> {
        match endpoint.scheme().to_lowercase().as_str() {
            "tcp" => {
                // Build a TCP dialer
                enforce_hostport!(endpoint);
                let variant = tcp::TcpDialer::new(None).await?;
                let variant = DialerVariant::Tcp(variant);
                Ok(Self { endpoint, variant })
            }

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
                let variant = tor::TorDialer::new(datastore).await?;
                let variant = DialerVariant::Tor(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-tor")]
            "tor+tls" => {
                // Build a Tor dialer wrapped with TLS
                enforce_hostport!(endpoint);
                let variant = tor::TorDialer::new(datastore).await?;
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
                // Build a Unix socket dialer
                enforce_abspath!(endpoint);
                let variant = unix::UnixDialer::new().await?;
                let variant = DialerVariant::Unix(variant);
                Ok(Self { endpoint, variant })
            }

            "socks5" => {
                // Build a SOCKS5 dialer
                enforce_hostport!(endpoint);
                let variant = socks5::Socks5Dialer::new(&endpoint).await?;
                let variant = DialerVariant::Socks5(variant);
                Ok(Self { endpoint, variant })
            }

            "socks5+tls" => {
                // Build a SOCKS5 dialer with TLS encapsulation
                enforce_hostport!(endpoint);
                let variant = socks5::Socks5Dialer::new(&endpoint).await?;
                let variant = DialerVariant::Socks5Tls(variant);
                Ok(Self { endpoint, variant })
            }

            x => {
                error!("[P2P] Requested unsupported transport: {}", x);
                Err(io::Error::from_raw_os_error(libc::ENETUNREACH))
            }
        }
    }

    /// Dial an instantiated [`Dialer`]. This creates a connection and returns a stream.
    /// The Tor-based Dialer variants can panic: this is intended. There exists validation
    /// for hosts and ports in other parts of the codebase. A panic occurring here
    /// likely indicates a configuration issue on the part of the user. It is preferable
    /// in this case that the user is alerted to this problem via a panic.
    pub async fn dial(&self, timeout: Option<Duration>) -> io::Result<Box<dyn PtStream>> {
        match &self.variant {
            DialerVariant::Tcp(dialer) => {
                // NOTE: sockaddr here is an array, can contain both ipv4 and ipv6
                let sockaddr = self.endpoint.socket_addrs(|| None)?;
                let stream = dialer.do_dial(sockaddr[0], timeout).await?;
                Ok(Box::new(stream))
            }

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
                let stream = dialer.do_dial(host, port, timeout).await?;
                Ok(Box::new(stream))
            }

            #[cfg(feature = "p2p-tor")]
            DialerVariant::TorTls(dialer) => {
                let host = self.endpoint.host_str().unwrap();
                let port = self.endpoint.port().unwrap();
                let stream = dialer.do_dial(host, port, timeout).await?;
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
                let path = match self.endpoint.to_file_path() {
                    Ok(v) => v,
                    Err(_) => return Err(io::Error::new(ErrorKind::Unsupported, "Invalid path")),
                };
                let stream = dialer.do_dial(path).await?;
                Ok(Box::new(stream))
            }

            DialerVariant::Socks5(dialer) => {
                let stream = dialer.do_dial().await?;
                Ok(Box::new(stream))
            }

            DialerVariant::Socks5Tls(dialer) => {
                let stream = dialer.do_dial().await?;
                let tlsupgrade = tls::TlsUpgrade::new().await;
                let stream = tlsupgrade.upgrade_dialer_tls(stream).await?;
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
    /// Instantiate a new [`Listener`] with the given [`Url`] and datastore path.
    /// Must contain a scheme, host string, and a port.
    pub async fn new(endpoint: Url, datastore: Option<String>) -> io::Result<Self> {
        match endpoint.scheme().to_lowercase().as_str() {
            "tcp" => {
                // Build a TCP listener
                enforce_hostport!(endpoint);
                let variant = tcp::TcpListener::new(1024).await?;
                let variant = ListenerVariant::Tcp(variant);
                Ok(Self { endpoint, variant })
            }

            "tcp+tls" => {
                // Build a TCP listener wrapped with TLS
                enforce_hostport!(endpoint);
                let variant = tcp::TcpListener::new(1024).await?;
                let variant = ListenerVariant::TcpTls(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-tor")]
            "tor" => {
                // Build a Tor Hidden Service listener
                enforce_hostport!(endpoint);
                let variant = tor::TorListener::new(datastore).await?;
                let variant = ListenerVariant::Tor(variant);
                Ok(Self { endpoint, variant })
            }

            #[cfg(feature = "p2p-unix")]
            "unix" => {
                enforce_abspath!(endpoint);
                let variant = unix::UnixListener::new().await?;
                let variant = ListenerVariant::Unix(variant);
                Ok(Self { endpoint, variant })
            }

            x => {
                error!("[P2P] Requested unsupported transport: {}", x);
                Err(io::Error::from_raw_os_error(libc::ENETUNREACH))
            }
        }
    }

    /// Listen on an instantiated [`Listener`].
    /// This will open a socket and return the listener.
    pub async fn listen(&self) -> io::Result<Box<dyn PtListener>> {
        match &self.variant {
            ListenerVariant::Tcp(listener) => {
                let sockaddr = self.endpoint.socket_addrs(|| None)?;
                let l = listener.do_listen(sockaddr[0]).await?;
                Ok(Box::new(l))
            }

            ListenerVariant::TcpTls(listener) => {
                let sockaddr = self.endpoint.socket_addrs(|| None)?;
                let l = listener.do_listen(sockaddr[0]).await?;
                let tlsupgrade = tls::TlsUpgrade::new().await;
                let l = tlsupgrade.upgrade_listener_tcp_tls(l).await?;
                Ok(Box::new(l))
            }

            #[cfg(feature = "p2p-tor")]
            ListenerVariant::Tor(listener) => {
                let port = self.endpoint.port().unwrap();
                let l = listener.do_listen(port).await?;
                Ok(Box::new(l))
            }

            #[cfg(feature = "p2p-unix")]
            ListenerVariant::Unix(listener) => {
                let path = match self.endpoint.to_file_path() {
                    Ok(v) => v,
                    Err(_) => return Err(io::Error::new(ErrorKind::Unsupported, "Invalid path")),
                };
                let l = listener.do_listen(&path).await?;
                Ok(Box::new(l))
            }
        }
    }

    /// Should only be called after `listen()` in order to behave correctly.
    pub async fn endpoint(&self) -> Url {
        match &self.variant {
            ListenerVariant::Tcp(listener) | ListenerVariant::TcpTls(listener) => {
                let mut endpoint = self.endpoint.clone();

                // Endpoint *must* always have a port set.
                // This is enforced by the enforce_hostport!() macro in Listener::new().
                let port = self.endpoint.port().unwrap();

                // `port == 0` means we got the OS to assign a random listen port to us.
                // Get the port from the listener and modify the endpoint.
                if port == 0 {
                    // Was `.listen()` called yet? Otherwise do nothing
                    if let Some(actual_port) = listener.port.get() {
                        endpoint.set_port(Some(*actual_port)).unwrap();
                    }
                }

                endpoint
            }
            #[cfg(feature = "p2p-tor")]
            ListenerVariant::Tor(listener) => listener.endpoint.get().unwrap().clone(),
            #[allow(unreachable_patterns)]
            _ => self.endpoint.clone(),
        }
    }
}

/// Wrapper trait for async streams
pub trait PtStream: AsyncRead + AsyncWrite + Unpin + Send {}

impl PtStream for smol::net::TcpStream {}

impl PtStream for futures_rustls::TlsStream<smol::net::TcpStream> {}

#[cfg(feature = "p2p-tor")]
impl PtStream for arti_client::DataStream {}

#[cfg(feature = "p2p-tor")]
impl PtStream for futures_rustls::TlsStream<arti_client::DataStream> {}

#[cfg(feature = "p2p-unix")]
impl PtStream for smol::net::unix::UnixStream {}

/// Wrapper trait for async listeners
#[async_trait]
pub trait PtListener: Send + Unpin {
    async fn next(&self) -> io::Result<(Box<dyn PtStream>, Url)>;
}
