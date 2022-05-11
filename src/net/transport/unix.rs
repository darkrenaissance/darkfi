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

#[derive(Copy, Clone)]
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
