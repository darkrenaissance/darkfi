use std::{io, net::SocketAddr, pin::Pin};

use async_std::net::{TcpListener, TcpStream};
use futures::prelude::*;
use log::debug;
use socket2::{Domain, Socket, Type};
use url::Url;

use super::{Transport, TransportError};

#[derive(Clone)]
pub struct TcpTransport {
    pub ttl: Option<u32>,
}

impl Transport for TcpTransport {
    type Acceptor = TcpListener;
    type Connector = TcpStream;

    type Error = io::Error;

    type Listener = Pin<Box<dyn Future<Output = Result<Self::Acceptor, Self::Error>> + Send>>;
    type Dial = Pin<Box<dyn Future<Output = Result<Self::Connector, Self::Error>> + Send>>;

    fn listen_on(self, url: Url) -> Result<Self::Listener, TransportError<Self::Error>> {
        if url.scheme() != "tcp" {
            return Err(TransportError::AddrNotSupported(url))
        }

        let socket_addr = url.socket_addrs(|| None)?[0];
        debug!(target: "tcptransport", "listening on {}", socket_addr);
        Ok(Box::pin(self.do_listen(socket_addr)))
    }

    fn dial(self, url: Url) -> Result<Self::Dial, TransportError<Self::Error>> {
        if url.scheme() != "tcp" {
            return Err(TransportError::AddrNotSupported(url))
        }

        let socket_addr = url.socket_addrs(|| None)?[0];
        debug!(target: "tcptransport", "dialing {}", socket_addr);
        Ok(Box::pin(self.do_dial(socket_addr)))
    }
}

impl TcpTransport {
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

    async fn do_listen(self, socket_addr: SocketAddr) -> Result<TcpListener, io::Error> {
        let socket = self.create_socket(socket_addr)?;
        socket.bind(&socket_addr.into())?;
        // TODO: make backlog configurable
        socket.listen(1024)?;
        socket.set_nonblocking(true)?;
        Ok(TcpListener::from(std::net::TcpListener::from(socket)))
    }

    async fn do_dial(self, socket_addr: SocketAddr) -> Result<TcpStream, io::Error> {
        let socket = self.create_socket(socket_addr)?;
        socket.set_nonblocking(true)?;

        match socket.connect(&socket_addr.into()) {
            Ok(()) => {}
            Err(err) if err.raw_os_error() == Some(libc::EINPROGRESS) => {}
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => return Err(err),
        };

        let stream = TcpStream::from(std::net::TcpStream::from(socket));
        Ok(stream)
    }
}
