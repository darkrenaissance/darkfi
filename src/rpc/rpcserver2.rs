use async_std::{
    io::{ReadExt, WriteExt},
    stream::StreamExt,
    sync::Arc,
};
use async_trait::async_trait;
use log::{debug, error, info};
use url::Url;

use super::jsonrpc::{JsonRequest, JsonResult};
use crate::{
    net::transport::{TcpTransport, TlsTransport, Transport},
    Error, Result,
};

#[async_trait]
pub trait RequestHandler: Sync + Send {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult;
}

pub async fn listen_and_serve(url: Url, rh: Arc<impl RequestHandler + 'static>) -> Result<()> {
    debug!(target: "JSON-RPC SERVER", "Trying to start listener on {}", url);

    macro_rules! handle_stream {
        ($stream:expr) => {
            let mut buf = vec![0; 8192];

            loop {
                let n = match $stream.read(&mut buf).await {
                    Ok(n) if n == 0 => {
                        info!(target: "JSON-RPC SERVER", "Closed connection");
                        break;
                    }
                    Ok(n) => n,
                    Err(e) => {
                        error!(target: "JSON-RPC SERVER", "Failed reading from socket: {}", e);
                        info!(target: "JSON-RPC SERVER", "Closed connection");
                        break;
                    }
                };

                let r: JsonRequest = match serde_json::from_slice(&buf[0..n]) {
                    Ok(r) => {
                        debug!(target: "JSON-RPC SERVER", "--> {}", String::from_utf8_lossy(&buf));
                        r
                    }
                    Err(e) => {
                        error!(target: "JSON-RPC SERVER", "Received invalid JSON: {:?}", e);
                        info!(target: "JSON-RPC SERVER", "Closed connection");
                        break;
                    }
                };

                let reply = rh.handle_request(r).await;
                let j = serde_json::to_string(&reply)?;
                debug!(target: "JSON-RPC SERVER", "<-- {}", j);

                if let Err(e) = $stream.write_all(j.as_bytes()).await {
                    error!(target: "JSON-RPC SERVER", "Failed writing to socket: {}", e);
                    info!(target: "JSON-RPC SERVER", "Closed connection");
                    break;
                }
            }
        }
    }

    match url.scheme() {
        "tcp" => {
            let transport = TcpTransport::new(None, 1024);
            let listener = transport.listen_on(url).unwrap().await.unwrap();
            let mut incoming = listener.incoming();
            while let Some(stream) = incoming.next().await {
                info!(target: "JSON-RPC SERVER", "Accepted TCP connection");
                let mut stream = stream.unwrap();
                handle_stream!(stream);
            }
            unreachable!()
        }

        "tls" => {
            let transport = TlsTransport::new(None, 1024);
            let (acceptor, listener) = transport.listen_on(url).unwrap().await.unwrap();
            let mut incoming = listener.incoming();
            while let Some(stream) = incoming.next().await {
                info!(target: "JSON-RPC SERVER", "Accepted TLS connection");
                let stream = stream.unwrap();
                let mut stream = acceptor.accept(stream).await.unwrap();
                handle_stream!(stream);
            }
            unreachable!()
        }

        "tor" => {
            todo!()
        }

        x => {
            error!(target: "JSON-RPC SERVER", "Transport protocol '{}' isn't implemented", x);
            Err(Error::UnsupportedTransport(x.to_string()))
        }
    }
}
