use async_std::net::{TcpListener, TcpStream};
use std::{io, net::SocketAddr, pin::Pin};

use futures::prelude::*;
use futures_rustls::{TlsAcceptor, TlsStream};
use log::debug;
use socket2::{Domain, Socket, Type};
use url::Url;

use super::{TlsUpgrade, Transport};
use crate::{Error, Result};

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
            "tcp" | "tcp+tls" => {}
            x => return Err(Error::UnsupportedTransport(x.to_string())),
        }

        let socket_addr = url.socket_addrs(|| None)?[0];
        debug!("{} transport: listening on {}", url.scheme(), socket_addr);
        Ok(Box::pin(self.do_listen(socket_addr)))
    }

    fn upgrade_listener(self, acceptor: Self::Acceptor) -> Result<Self::TlsListener> {
        let tlsupgrade = TlsUpgrade::new();
        Ok(Box::pin(tlsupgrade.upgrade_listener_tls(acceptor)))
    }

    fn dial(self, url: Url) -> Result<Self::Dial> {
        match url.scheme() {
            "tcp" | "tcp+tls" => {}
            x => return Err(Error::UnsupportedTransport(x.to_string())),
        }

        let socket_addr = url.socket_addrs(|| None)?[0];
        debug!("{} transport: dialing {}", url.scheme(), socket_addr);
        Ok(Box::pin(self.do_dial(socket_addr)))
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

        Ok(socket)
    }

    async fn do_listen(self, socket_addr: SocketAddr) -> Result<TcpListener> {
        let socket = self.create_socket(socket_addr)?;
        socket.bind(&socket_addr.into())?;
        socket.listen(self.backlog)?;
        socket.set_nonblocking(true)?;
        Ok(TcpListener::from(std::net::TcpListener::from(socket)))
    }

    async fn do_dial(self, socket_addr: SocketAddr) -> Result<TcpStream> {
        let socket = self.create_socket(socket_addr)?;
        socket.set_nonblocking(true)?;

        match socket.connect(&socket_addr.into()) {
            Ok(()) => {}
            Err(err) if err.raw_os_error() == Some(libc::EINPROGRESS) => {}
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => return Err(err.into()),
        };

        let stream = TcpStream::from(std::net::TcpStream::from(socket));
        Ok(stream)
    }
}
