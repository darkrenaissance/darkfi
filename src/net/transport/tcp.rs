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

use std::{io, sync::Arc, time::Duration};

use async_trait::async_trait;
use futures::{
    future::{select, Either},
    pin_mut,
};
use futures_rustls::{TlsAcceptor, TlsStream};
use log::debug;
use smol::{
    lock::OnceCell,
    net::{SocketAddr, TcpListener as SmolTcpListener, TcpStream},
    Async, Timer,
};
use socket2::{Domain, Socket, TcpKeepalive, Type};
use url::Url;

use super::{PtListener, PtStream};

trait SocketExt {
    fn enable_reuse_port(&self) -> io::Result<()>;
}

impl SocketExt for Socket {
    fn enable_reuse_port(&self) -> io::Result<()> {
        #[cfg(target_family = "unix")]
        self.set_reuse_port(true)?;

        // On Windows SO_REUSEPORT means the same thing as SO_REUSEADDR
        #[cfg(target_family = "windows")]
        self.set_reuse_address(true)?;

        Ok(())
    }
}

/// TCP Dialer implementation
#[derive(Debug, Clone)]
pub struct TcpDialer {
    /// TTL to set for opened sockets, or `None` for default.
    ttl: Option<u32>,
}

impl TcpDialer {
    /// Instantiate a new [`TcpDialer`] with optional TTL.
    pub(crate) async fn new(ttl: Option<u32>) -> io::Result<Self> {
        Ok(Self { ttl })
    }

    /// Internal helper function to create a TCP socket.
    async fn create_socket(&self, socket_addr: SocketAddr) -> io::Result<Socket> {
        let domain = if socket_addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
        let socket = Socket::new(domain, Type::STREAM, Some(socket2::Protocol::TCP))?;

        if socket_addr.is_ipv6() {
            socket.set_only_v6(true)?;
        }

        if let Some(ttl) = self.ttl {
            socket.set_ttl_v4(ttl)?;
        }

        socket.set_tcp_nodelay(true)?;
        let keepalive = TcpKeepalive::new().with_time(Duration::from_secs(20));
        socket.set_tcp_keepalive(&keepalive)?;
        socket.enable_reuse_port()?;

        Ok(socket)
    }

    /// Internal dial function
    pub(crate) async fn do_dial(
        &self,
        socket_addr: SocketAddr,
        timeout: Option<Duration>,
    ) -> io::Result<TcpStream> {
        debug!(target: "net::tcp::do_dial", "Dialing {socket_addr} with TCP...");
        let socket = self.create_socket(socket_addr).await?;

        socket.set_nonblocking(true)?;

        // Sync start socket connect. A WouldBlock error means this
        // connection is in progress.
        match socket.connect(&socket_addr.into()) {
            Ok(()) => {}
            Err(err) if err.raw_os_error() == Some(libc::EINPROGRESS) => {}
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => return Err(err),
        };

        let stream = Async::new_nonblocking(std::net::TcpStream::from(socket))?;

        // Wait until the async object becomes writable.
        let connect = async move {
            stream.writable().await?;
            match stream.get_ref().take_error()? {
                Some(err) => Err(err),
                None => Ok(stream),
            }
        };

        // If a timeout is configured, run both the connect and timeout
        // futures and return whatever finishes first. Otherwise wait on
        // the connect future.
        match timeout {
            Some(t) => {
                let timeout = Timer::after(t);
                pin_mut!(timeout);
                pin_mut!(connect);

                match select(connect, timeout).await {
                    Either::Left((Ok(stream), _)) => Ok(TcpStream::from(stream)),
                    Either::Left((Err(e), _)) => Err(e),
                    Either::Right((_, _)) => Err(io::ErrorKind::TimedOut.into()),
                }
            }
            None => {
                let stream = connect.await?;
                Ok(TcpStream::from(stream))
            }
        }
    }
}

/// TCP Listener implementation
#[derive(Debug, Clone)]
pub struct TcpListener {
    /// Size of the listen backlog for listen sockets
    backlog: i32,
    /// When the user puts a port of 0, the OS will assign a random port.
    /// We get it from the listener so we know what the true endpoint is.
    pub port: Arc<OnceCell<u16>>,
}

impl TcpListener {
    /// Instantiate a new [`TcpListener`] with given backlog size.
    pub async fn new(backlog: i32) -> io::Result<Self> {
        Ok(Self { backlog, port: Arc::new(OnceCell::new()) })
    }

    /// Internal helper function to create a TCP socket.
    async fn create_socket(&self, socket_addr: SocketAddr) -> io::Result<Socket> {
        let domain = if socket_addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
        let socket = Socket::new(domain, Type::STREAM, Some(socket2::Protocol::TCP))?;

        if socket_addr.is_ipv6() {
            socket.set_only_v6(true)?;
        }

        socket.set_tcp_nodelay(true)?;
        let keepalive = TcpKeepalive::new().with_time(Duration::from_secs(20));
        socket.set_tcp_keepalive(&keepalive)?;
        socket.enable_reuse_port()?;

        Ok(socket)
    }

    /// Internal listen function
    pub(crate) async fn do_listen(&self, socket_addr: SocketAddr) -> io::Result<SmolTcpListener> {
        let socket = self.create_socket(socket_addr).await?;
        socket.bind(&socket_addr.into())?;
        socket.listen(self.backlog)?;
        socket.set_nonblocking(true)?;

        let listener = std::net::TcpListener::from(socket);
        let local_port = listener.local_addr()?.port();
        let listener = smol::Async::<std::net::TcpListener>::try_from(listener)?;

        self.port.set(local_port).await.expect("fatal port already set for TcpListener");

        Ok(SmolTcpListener::from(listener))
    }
}

#[async_trait]
impl PtListener for SmolTcpListener {
    async fn next(&self) -> io::Result<(Box<dyn PtStream>, Url)> {
        let (stream, peer_addr) = match self.accept().await {
            Ok((s, a)) => (s, a),
            Err(e) => return Err(e),
        };

        let url = Url::parse(&format!("tcp://{peer_addr}")).unwrap();
        Ok((Box::new(stream), url))
    }
}

#[async_trait]
impl PtListener for (TlsAcceptor, SmolTcpListener) {
    async fn next(&self) -> io::Result<(Box<dyn PtStream>, Url)> {
        let (stream, peer_addr) = match self.1.accept().await {
            Ok((s, a)) => (s, a),
            Err(e) => return Err(e),
        };

        let stream = match self.0.accept(stream).await {
            Ok(v) => v,
            Err(e) => return Err(e),
        };

        let url = Url::parse(&format!("tcp+tls://{peer_addr}")).unwrap();

        Ok((Box::new(TlsStream::Server(stream)), url))
    }
}
