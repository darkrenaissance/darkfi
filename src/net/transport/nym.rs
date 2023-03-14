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

use std::{io, net::SocketAddr, pin::Pin, time::Duration};

use async_std::net::{TcpListener, TcpStream};
use fast_socks5::client::{Config, Socks5Stream};
use futures::prelude::*;
use futures_rustls::{TlsAcceptor, TlsStream};
use socket2::{Domain, Socket, TcpKeepalive, Type};
use url::Url;

use crate::{Error, Result};

use super::{TlsUpgrade, Transport};

#[derive(Clone)]
pub struct NymTransport {
    socks_url: Url,
}

impl NymTransport {
    pub fn new() -> Result<Self> {
        let socks_url = Url::parse("socks5://127.0.0.1:1080")?;
        Ok(Self { socks_url })
    }

    pub async fn do_dial(self, url: Url) -> Result<Socks5Stream<TcpStream>> {
        let socks_url_str = self.socks_url.socket_addrs(|| None)?[0].to_string();
        let host = url.host().unwrap().to_string();
        let port = url.port().unwrap_or(80);
        let config = Config::default();
        let stream = if !self.socks_url.username().is_empty() && self.socks_url.password().is_some()
        {
            Socks5Stream::connect_with_password(
                socks_url_str,
                host,
                port,
                self.socks_url.username().to_string(),
                self.socks_url.password().unwrap().to_string(),
                config,
            )
            .await?
        } else {
            Socks5Stream::connect(socks_url_str, host, port, config).await?
        };
        Ok(stream)
    }

    fn create_socket(&self, socket_addr: SocketAddr) -> io::Result<Socket> {
        let domain = if socket_addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
        let socket = Socket::new(domain, Type::STREAM, Some(socket2::Protocol::TCP))?;

        if socket_addr.is_ipv6() {
            socket.set_only_v6(true)?;
        }

        // TODO: Perhaps make these configurable
        socket.set_nodelay(true)?;
        let keepalive = TcpKeepalive::new().with_time(Duration::from_secs(30));
        socket.set_tcp_keepalive(&keepalive)?;
        // TODO: Make sure to disallow running multiple instances of a program using this.
        socket.set_reuse_port(true)?;

        Ok(socket)
    }

    pub async fn do_listen(self, url: Url) -> Result<TcpListener> {
        let socket_addr = url.socket_addrs(|| None)?[0];
        let socket = self.create_socket(socket_addr)?;
        socket.bind(&socket_addr.into())?;
        socket.listen(1024)?;
        socket.set_nonblocking(true)?;
        Ok(TcpListener::from(std::net::TcpListener::from(socket)))
    }
}

impl Transport for NymTransport {
    type Acceptor = TcpListener;
    type Connector = Socks5Stream<TcpStream>;

    type Listener = Pin<Box<dyn Future<Output = Result<Self::Acceptor>> + Send>>;
    type Dial = Pin<Box<dyn Future<Output = Result<Self::Connector>> + Send>>;

    type TlsListener = Pin<Box<dyn Future<Output = Result<(TlsAcceptor, Self::Acceptor)>> + Send>>;
    type TlsDialer = Pin<Box<dyn Future<Output = Result<TlsStream<Self::Connector>>> + Send>>;

    fn listen_on(self, url: Url) -> Result<Self::Listener> {
        match url.scheme() {
            "nym" | "nym+tls" => {}
            x => return Err(Error::UnsupportedTransport(x.to_string())),
        }
        Ok(Box::pin(self.do_listen(url)))
    }

    fn upgrade_listener(self, acceptor: Self::Acceptor) -> Result<Self::TlsListener> {
        let tlsupgrade = TlsUpgrade::new();
        Ok(Box::pin(tlsupgrade.upgrade_listener_tls(acceptor)))
    }

    fn dial(self, url: Url, _timeout: Option<Duration>) -> Result<Self::Dial> {
        match url.scheme() {
            "nym" | "nym+tls" => {}
            x => return Err(Error::UnsupportedTransport(x.to_string())),
        }
        println!("using nym transport=======================================================");
        Ok(Box::pin(self.do_dial(url)))
    }

    fn upgrade_dialer(self, connector: Self::Connector) -> Result<Self::TlsDialer> {
        let tlsupgrade = TlsUpgrade::new();
        Ok(Box::pin(tlsupgrade.upgrade_dialer_tls(connector)))
    }
}
