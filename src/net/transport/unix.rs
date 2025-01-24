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

use std::{
    io,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use log::debug;
use smol::{
    fs,
    net::unix::{UnixListener as SmolUnixListener, UnixStream},
};
use url::Url;

use super::{PtListener, PtStream};

/// Unix Dialer implementation
#[derive(Debug, Clone)]
pub struct UnixDialer;

impl UnixDialer {
    /// Instantiate a new [`UnixDialer`] object
    pub(crate) async fn new() -> io::Result<Self> {
        Ok(Self {})
    }

    /// Internal dial function
    pub(crate) async fn do_dial(
        &self,
        path: impl AsRef<Path> + core::fmt::Debug,
    ) -> io::Result<UnixStream> {
        debug!(target: "net::unix::do_dial", "Dialing {:?} Unix socket...", path);
        let stream = UnixStream::connect(path).await?;
        Ok(stream)
    }
}

/// Unix Listener implementation
#[derive(Debug, Clone)]
pub struct UnixListener;

impl UnixListener {
    /// Instantiate a new [`UnixListener`] object
    pub(crate) async fn new() -> io::Result<Self> {
        Ok(Self {})
    }

    /// Internal listen function
    pub(crate) async fn do_listen(&self, path: &PathBuf) -> io::Result<SmolUnixListener> {
        // This rm is a bit aggressive, but c'est la vie.
        let _ = fs::remove_file(path).await;
        let listener = SmolUnixListener::bind(path)?;
        Ok(listener)
    }
}

#[async_trait]
impl PtListener for SmolUnixListener {
    async fn next(&self) -> io::Result<(Box<dyn PtStream>, Url)> {
        let (stream, _peer_addr) = match self.accept().await {
            Ok((s, a)) => (s, a),
            Err(e) => return Err(e),
        };

        let addr = self.local_addr().unwrap();
        let addr = addr.as_pathname().unwrap().to_str().unwrap();
        let url = Url::parse(&format!("unix://{}", addr)).unwrap();

        Ok((Box::new(stream), url))
    }
}
