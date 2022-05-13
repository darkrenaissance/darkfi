use async_std::{
    io::{ReadExt, WriteExt},
    sync::Arc,
};
use std::{env, fs};

use async_trait::async_trait;
use log::{debug, error, info};
use url::Url;

use super::jsonrpc::{JsonRequest, JsonResult};
use crate::{
    net::transport::{
        TcpTransport, TorTransport, Transport, TransportListener, TransportName, TransportStream,
        UnixTransport,
    },
    Error, Result,
};

#[async_trait]
pub trait RequestHandler: Sync + Send {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult;
}

async fn run_accept_loop(
    listener: Box<dyn TransportListener>,
    rh: Arc<impl RequestHandler + 'static>,
) -> Result<()> {
    while let Ok((stream, peer_addr)) = listener.next().await {
        info!(target: "JSON-RPC SERVER", "RPC Accepted connection {}", peer_addr);
        accept(stream, rh.clone()).await?;
    }
    Ok(())
}

async fn accept(
    mut stream: Box<dyn TransportStream>,
    rh: Arc<impl RequestHandler + 'static>,
) -> Result<()> {
    let mut buf = vec![0; 8192];

    loop {
        let n = match stream.read(&mut buf).await {
            Ok(n) if n == 0 => {
                info!(target: "JSON-RPC SERVER", "Closed connection");
                break
            }
            Ok(n) => n,
            Err(e) => {
                error!(target: "JSON-RPC SERVER", "Failed reading from socket: {}", e);
                info!(target: "JSON-RPC SERVER", "Closed connection");
                break
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
                break
            }
        };

        let reply = rh.handle_request(r).await;
        let j = serde_json::to_string(&reply)?;
        debug!(target: "JSON-RPC SERVER", "<-- {}", j);

        if let Err(e) = stream.write_all(j.as_bytes()).await {
            error!(target: "JSON-RPC SERVER", "Failed writing to socket: {}", e);
            info!(target: "JSON-RPC SERVER", "Closed connection");
            break
        }
    }

    Ok(())
}

pub async fn listen_and_serve(
    accept_url: Url,
    rh: Arc<impl RequestHandler + 'static>,
) -> Result<()> {
    debug!(target: "JSON-RPC SERVER", "Trying to start listener on {}", accept_url);

    macro_rules! accept {
        ($listener:expr, $transport:expr, $upgrade:expr) => {{
            if let Err(err) = $listener {
                error!("Setup for {} failed: {}", accept_url, err);
                return Err(Error::BindFailed(accept_url.as_str().into()))
            }

            let listener = $listener?.await;

            if let Err(err) = listener {
                error!("Bind listener to {} failed: {}", accept_url, err);
                return Err(Error::BindFailed(accept_url.as_str().into()))
            }

            let listener = listener?;

            match $upgrade {
                None => {
                    info!("RPC listening to: {}", accept_url);
                    run_accept_loop(Box::new(listener), rh).await?;
                }
                Some(u) if u == "tls" => {
                    info!("RPC listening to: {}", accept_url);
                    let tls_listener = $transport.upgrade_listener(listener)?.await?;
                    run_accept_loop(Box::new(tls_listener), rh).await?;
                }
                Some(u) => return Err(Error::UnsupportedTransportUpgrade(u)),
            }
        }};
    }

    let transport_name = TransportName::try_from(accept_url.clone())?;
    match transport_name {
        TransportName::Tcp(upgrade) => {
            let transport = TcpTransport::new(None, 1024);
            let listener = transport.listen_on(accept_url.clone());
            accept!(listener, transport, upgrade);
        }
        TransportName::Tor(upgrade) => {
            let socks5_url = Url::parse(
                &env::var("DARKFI_TOR_SOCKS5_URL")
                    .unwrap_or_else(|_| "socks5://127.0.0.1:9050".to_string()),
            )?;

            let torc_url = Url::parse(
                &env::var("DARKFI_TOR_CONTROL_URL")
                    .unwrap_or_else(|_| "tcp://127.0.0.1:9051".to_string()),
            )?;

            let auth_cookie = env::var("DARKFI_TOR_COOKIE");

            if auth_cookie.is_err() {
                return Err(Error::TorError(
                    "Please set the env var DARKFI_TOR_COOKIE to the configured tor cookie file. \
                    For example: \
                    \'export DARKFI_TOR_COOKIE=\"/var/lib/tor/control_auth_cookie\"\'"
                        .to_string(),
                ))
            }

            let auth_cookie = auth_cookie.unwrap();

            let auth_cookie = hex::encode(&fs::read(auth_cookie).unwrap());

            let transport = TorTransport::new(socks5_url, Some((torc_url, auth_cookie)))?;

            // generate EHS pointing to local address
            let hurl = transport.create_ehs(accept_url.clone())?;

            info!("EHS TOR: {}", hurl.to_string());

            let listener = transport.clone().listen_on(accept_url.clone());

            accept!(listener, transport, upgrade);
        }

        TransportName::Unix => {
            let transport = UnixTransport::new();

            let listener = transport.listen(accept_url.clone()).await;

            if let Err(err) = listener {
                error!("RPC Bind listener to {} failed: {}", accept_url, err);
                return Err(Error::BindFailed(accept_url.as_str().into()))
            }

            run_accept_loop(Box::new(listener?), rh).await?;
        }
        _ => unimplemented!(),
    }

    Ok(())
}
