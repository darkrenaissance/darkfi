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

use async_std::os::unix::net::{UnixListener, UnixStream};

use async_trait::async_trait;
use log::{debug, error};
use url::Url;

use super::{TransportListener, TransportStream};
use crate::{Error, Result};

fn unix_socket_addr_to_string(addr: std::os::unix::net::SocketAddr) -> String {
    addr.as_pathname().unwrap_or(&std::path::PathBuf::from("")).to_str().unwrap_or("").into()
}

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

impl TransportStream for UnixStream {}

#[derive(Default, Copy, Clone)]
pub struct UnixTransport {}

impl UnixTransport {
    pub fn new() -> Self {
        Self {}
    }
    pub async fn listen(self, url: Url) -> Result<UnixListener> {
        match url.scheme() {
            "unix" => {}
            x => return Err(Error::UnsupportedTransport(x.to_string())),
        }

        if !cfg!(unix) {
            return Err(Error::UnsupportedOS)
        }

        let listener = UnixListener::bind(url.as_str()).await?;
        debug!("{} transport: listening on {}", url.scheme(), url);
        Ok(listener)
    }

    pub async fn dial(self, url: Url) -> Result<UnixStream> {
        match url.scheme() {
            "unix" => {}
            x => return Err(Error::UnsupportedTransport(x.to_string())),
        }

        if !cfg!(unix) {
            return Err(Error::UnsupportedOS)
        }

        let stream = UnixStream::connect(url.as_str()).await?;
        debug!("{} transport: dialing to {}", url.scheme(), url);
        Ok(stream)
    }
}
