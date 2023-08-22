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

use async_rustls::{TlsAcceptor, TlsStream};
use async_trait::async_trait;
use log::debug;
use smol::net::{SocketAddr, TcpListener as SmolTcpListener, TcpStream};
use url::Url;

use super::{PtListener, PtStream};
use crate::{system::io_timeout, Result};

/// TCP Dialer implementation
#[derive(Debug, Clone)]
pub struct TcpDialer {
    /// TTL to set for opened sockets, or `None` for default.
    ttl: Option<u32>,
}

impl TcpDialer {
    /// Instantiate a new [`TcpDialer`] with optional TTL.
    pub(crate) async fn new(ttl: Option<u32>) -> Result<Self> {
        Ok(Self { ttl })
    }

    /// Internal dial function
    pub(crate) async fn do_dial(
        &self,
        socket_addr: SocketAddr,
        conn_timeout: Option<Duration>,
    ) -> Result<TcpStream> {
        debug!(target: "net::tcp::do_dial", "Dialing {} with TCP...", socket_addr);
        let stream = if let Some(conn_timeout) = conn_timeout {
            io_timeout(conn_timeout, TcpStream::connect(socket_addr)).await?
        } else {
            TcpStream::connect(socket_addr).await?
        };

        if let Some(ttl) = self.ttl {
            stream.set_ttl(ttl)?;
        }

        stream.set_nodelay(true)?;

        Ok(stream)
    }
}

/// TCP Listener implementation
#[derive(Debug, Clone)]
pub struct TcpListener;

impl TcpListener {
    /// Instantiate a new [`TcpListener`] with given backlog size.
    pub async fn new() -> Result<Self> {
        Ok(Self {})
    }

    /// Internal listen function
    pub(crate) async fn do_listen(&self, socket_addr: SocketAddr) -> Result<SmolTcpListener> {
        let listener = SmolTcpListener::bind(socket_addr).await?;
        Ok(listener)
    }
}

#[async_trait]
impl PtListener for SmolTcpListener {
    async fn next(&self) -> Result<(Box<dyn PtStream>, Url)> {
        let (stream, peer_addr) = match self.accept().await {
            Ok((s, a)) => (s, a),
            Err(e) => return Err(e.into()),
        };

        let url = Url::parse(&format!("tcp://{}", peer_addr))?;
        Ok((Box::new(stream), url))
    }
}

#[async_trait]
impl PtListener for (TlsAcceptor, SmolTcpListener) {
    async fn next(&self) -> Result<(Box<dyn PtStream>, Url)> {
        let (stream, peer_addr) = match self.1.accept().await {
            Ok((s, a)) => (s, a),
            Err(e) => return Err(e.into()),
        };

        let stream = self.0.accept(stream).await;

        let url = Url::parse(&format!("tcp+tls://{}", peer_addr))?;

        if let Err(e) = stream {
            return Err(e.into())
        }

        Ok((Box::new(TlsStream::Server(stream.unwrap())), url))
    }
}
