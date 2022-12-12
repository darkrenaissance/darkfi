/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use async_std::net::{TcpListener, TcpStream};
use std::{io, net::SocketAddr, pin::Pin, time::Duration};

use async_trait::async_trait;
use futures::prelude::*;
use futures_rustls::{TlsAcceptor, TlsStream};
use log::{debug, error};
use socket2::{Domain, Socket, TcpKeepalive, Type};
use url::Url;

use super::{socket_addr_to_url, TlsUpgrade, Transport, TransportListener, TransportStream};
use crate::{Error, Result};

impl TransportStream for TcpStream {}
impl<T: TransportStream> TransportStream for TlsStream<T> {}

#[async_trait]
impl TransportListener for TcpListener {
    async fn next(&self) -> Result<(Box<dyn TransportStream>, Url)> {
        let (stream, peer_addr) = match self.accept().await {
            Ok((s, a)) => (s, a),
            Err(err) => {
                error!("Error listening for connections: {}", err);
                return Err(Error::AcceptConnectionFailed(self.local_addr()?.to_string()))
            }
        };
        let url = socket_addr_to_url(peer_addr, "tcp")?;
        Ok((Box::new(stream), url))
    }
}

#[async_trait]
impl TransportListener for (TlsAcceptor, TcpListener) {
    async fn next(&self) -> Result<(Box<dyn TransportStream>, Url)> {
        let (stream, peer_addr) = match self.1.accept().await {
            Ok((s, a)) => (s, a),
            Err(err) => {
                error!("Error listening for connections: {}", err);
                return Err(Error::AcceptConnectionFailed(self.1.local_addr()?.to_string()))
            }
        };

        let stream = self.0.accept(stream).await;

        let url = socket_addr_to_url(peer_addr, "tcp+tls")?;

        if let Err(err) = stream {
            error!("Error wrapping the connection {} with tls: {}", url, err);
            return Err(Error::AcceptTlsConnectionFailed(self.1.local_addr()?.to_string()))
        }

        Ok((Box::new(TlsStream::Server(stream?)), url))
    }
}

#[derive(Copy, Clone)]
pub struct TcpTransport {
    /// TTL to set for opened sockets, or `None` for default
    ttl: Option<u32>,
    /// Size of the listen backlog for listen sockets
    backlog: i32,
}

impl Transport for TcpTransport {
    type Acceptor = TcpListener;
    type Connector = TcpStream;

    type Listener = Pin<Box<dyn Future<Output = Result<Self::Acceptor>> + Send>>;
    type Dial = Pin<Box<dyn Future<Output = Result<Self::Connector>> + Send>>;

    type TlsListener = Pin<Box<dyn Future<Output = Result<(TlsAcceptor, Self::Acceptor)>> + Send>>;
    type TlsDialer = Pin<Box<dyn Future<Output = Result<TlsStream<Self::Connector>>> + Send>>;

    fn listen_on(self, url: Url) -> Result<Self::Listener> {
        match url.scheme() {
            "tcp" | "tcp+tls" | "tls" => {}
            x => return Err(Error::UnsupportedTransport(x.to_string())),
        }

        let socket_addr = url.socket_addrs(|| None)?[0];
        debug!(target: "net", "{} transport: listening on {}", url.scheme(), socket_addr);
        Ok(Box::pin(self.do_listen(socket_addr)))
    }

    fn upgrade_listener(self, acceptor: Self::Acceptor) -> Result<Self::TlsListener> {
        let tlsupgrade = TlsUpgrade::new();
        Ok(Box::pin(tlsupgrade.upgrade_listener_tls(acceptor)))
    }

    fn dial(self, url: Url, timeout: Option<Duration>) -> Result<Self::Dial> {
        match url.scheme() {
            "tcp" | "tcp+tls" | "tls" => {}
            x => return Err(Error::UnsupportedTransport(x.to_string())),
        }

        let socket_addr = url.socket_addrs(|| None)?[0];
        debug!(target: "net", "{} transport: dialing {}", url.scheme(), socket_addr);
        Ok(Box::pin(self.do_dial(socket_addr, timeout)))
    }

    fn upgrade_dialer(self, connector: Self::Connector) -> Result<Self::TlsDialer> {
        let tlsupgrade = TlsUpgrade::new();
        Ok(Box::pin(tlsupgrade.upgrade_dialer_tls(connector)))
    }
}

impl TcpTransport {
    pub fn new(ttl: Option<u32>, backlog: i32) -> Self {
        Self { ttl, backlog }
    }

    fn create_socket(&self, socket_addr: SocketAddr) -> io::Result<Socket> {
        let domain = if socket_addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
        let socket = Socket::new(domain, Type::STREAM, Some(socket2::Protocol::TCP))?;

        if socket_addr.is_ipv6() {
            socket.set_only_v6(true)?;
        }

        if let Some(ttl) = self.ttl {
            socket.set_ttl(ttl)?;
        }

        // TODO: Perhaps make these configurable
        socket.set_nodelay(true)?;
        let keepalive = TcpKeepalive::new().with_time(Duration::from_secs(20));
        socket.set_tcp_keepalive(&keepalive)?;
        // TODO: Make sure to disallow running multiple instances of a program using this.
        socket.set_reuse_port(true)?;

        Ok(socket)
    }

    async fn do_listen(self, socket_addr: SocketAddr) -> Result<TcpListener> {
        let socket = self.create_socket(socket_addr)?;
        socket.bind(&socket_addr.into())?;
        socket.listen(self.backlog)?;
        socket.set_nonblocking(true)?;
        Ok(TcpListener::from(std::net::TcpListener::from(socket)))
    }

    async fn do_dial(
        self,
        socket_addr: SocketAddr,
        timeout: Option<Duration>,
    ) -> Result<TcpStream> {
        let socket = self.create_socket(socket_addr)?;

        let connection = if timeout.is_some() {
            socket.connect_timeout(&socket_addr.into(), timeout.unwrap())
        } else {
            socket.connect(&socket_addr.into())
        };

        match connection {
            Ok(()) => {}
            Err(err) if err.raw_os_error() == Some(libc::EINPROGRESS) => {}
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => return Err(err.into()),
        };

        socket.set_nonblocking(true)?;
        let stream = TcpStream::from(std::net::TcpStream::from(socket));
        Ok(stream)
    }
}
