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

use std::{os::unix::net::SocketAddr, pin::Pin, time::Duration};

use async_std::os::unix::net::{UnixListener, UnixStream};
use async_trait::async_trait;
use futures::prelude::*;
use futures_rustls::{TlsAcceptor, TlsStream};
use log::{debug, error};
use url::Url;

use super::{Transport, TransportListener, TransportStream};
use crate::{Error, Result};

fn unix_socket_addr_to_string(addr: std::os::unix::net::SocketAddr) -> String {
    addr.as_pathname().unwrap_or(&std::path::PathBuf::from("")).to_str().unwrap_or("").into()
}

impl TransportStream for UnixStream {}

#[async_trait]
impl TransportListener for UnixListener {
    async fn next(&self) -> Result<(Box<dyn TransportStream>, Url)> {
        let (stream, peer_addr) = match self.accept().await {
            Ok((s, a)) => (s, a),
            Err(err) => {
                error!("Error listening for connections: {}", err);
                return Err(Error::AcceptConnectionFailed(unix_socket_addr_to_string(
                    self.local_addr()?,
                )))
            }
        };
        let url = Url::parse(&unix_socket_addr_to_string(peer_addr))?;
        Ok((Box::new(stream), url))
    }
}

#[async_trait]
impl TransportListener for (TlsAcceptor, UnixListener) {
    async fn next(&self) -> Result<(Box<dyn TransportStream>, Url)> {
        unimplemented!("TLS not supported for Unix sockets");
    }
}

#[derive(Copy, Clone)]
pub struct UnixTransport {}

impl Transport for UnixTransport {
    type Acceptor = UnixListener;
    type Connector = UnixStream;

    type Listener = Pin<Box<dyn Future<Output = Result<Self::Acceptor>> + Send>>;
    type Dial = Pin<Box<dyn Future<Output = Result<Self::Connector>> + Send>>;

    type TlsListener = Pin<Box<dyn Future<Output = Result<(TlsAcceptor, Self::Acceptor)>> + Send>>;
    type TlsDialer = Pin<Box<dyn Future<Output = Result<TlsStream<Self::Connector>>> + Send>>;

    fn listen_on(self, url: Url) -> Result<Self::Listener> {
        match url.scheme() {
            "unix" => {}
            x => return Err(Error::UnsupportedTransport(x.to_string())),
        }

        let socket_path = url.path();
        let socket_addr = SocketAddr::from_pathname(&socket_path)?;
        debug!(target: "net", "{} transport: listening on {}", url.scheme(), socket_path);
        Ok(Box::pin(self.do_listen(socket_addr)))
    }

    fn upgrade_listener(self, _acceptor: Self::Acceptor) -> Result<Self::TlsListener> {
        unimplemented!("TLS not supported for Unix sockets");
    }

    fn dial(self, url: Url, timeout: Option<Duration>) -> Result<Self::Dial> {
        match url.scheme() {
            "unix" => {}
            x => return Err(Error::UnsupportedTransport(x.to_string())),
        }

        let socket_path = url.path();
        let socket_addr = SocketAddr::from_pathname(&socket_path)?;
        debug!(target: "net", "{} transport: listening on {}", url.scheme(), socket_path);
        Ok(Box::pin(self.do_dial(socket_addr, timeout)))
    }

    fn upgrade_dialer(self, _connector: Self::Connector) -> Result<Self::TlsDialer> {
        unimplemented!("TLS not supported for Unix sockets");
    }
}

impl UnixTransport {
    pub fn new() -> Self {
        Self {}
    }

    async fn do_listen(self, socket_addr: SocketAddr) -> Result<UnixListener> {
        // We're a bit rough here and delete the socket.
        let socket_path = socket_addr.as_pathname().unwrap();
        if std::fs::metadata(socket_path).is_ok() {
            std::fs::remove_file(socket_path)?;
        }

        let socket = UnixListener::bind(socket_path).await?;
        Ok(socket)
    }

    async fn do_dial(
        self,
        socket_addr: SocketAddr,
        _timeout: Option<Duration>,
    ) -> Result<UnixStream> {
        let socket_path = socket_addr.as_pathname().unwrap();
        let stream = UnixStream::connect(&socket_path).await?;
        Ok(stream)
    }
}
