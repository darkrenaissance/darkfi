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

//! JSON-RPC server-side implementation.
use async_std::sync::Arc;
use async_trait::async_trait;
use futures::{AsyncReadExt, AsyncWriteExt};
use log::{debug, error, info, warn};
use url::Url;

use super::jsonrpc::{JsonRequest, JsonResult};
use crate::{
    net::transport::{
        TcpTransport, TorTransport, Transport, TransportListener, TransportName, TransportStream,
        UnixTransport,
    },
    Error, Result,
};

/// Asynchronous trait implementing a handler for incoming JSON-RPC requests.
/// Can be used by matching on methods and branching out to functions that
/// handle respective methods.
#[async_trait]
pub trait RequestHandler: Sync + Send {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult;
}

/// Internal accept function that runs inside a loop for accepting incoming
/// JSON-RPC requests and passing them to the [`RequestHandler`].
async fn accept(
    mut stream: Box<dyn TransportStream>,
    peer_addr: Url,
    rh: Arc<impl RequestHandler + 'static>,
) -> Result<()> {
    loop {
        // FIXME: Nasty size. 8M
        let mut buf = vec![0; 1024 * 8192];

        let n = match stream.read(&mut buf).await {
            Ok(n) if n == 0 => {
                debug!(target: "rpc::server", "Closed connection for {}", peer_addr);
                break
            }
            Ok(n) => n,
            Err(e) => {
                error!(target: "rpc::server", "JSON-RPC server failed reading from {} socket: {}", peer_addr, e);
                debug!(target: "rpc::server", "Closed connection for {}", peer_addr);
                break
            }
        };

        let r: JsonRequest = match serde_json::from_slice(&buf[0..n]) {
            Ok(r) => {
                debug!(target: "rpc::server", "{} --> {}", peer_addr, String::from_utf8_lossy(&buf));
                r
            }
            Err(e) => {
                warn!(target: "rpc::server", "JSON-RPC server received invalid JSON from {}: {}", peer_addr, e);
                debug!(target: "rpc::server", "Closed connection for {}", peer_addr);
                break
            }
        };

        let reply = rh.handle_request(r).await;
        match reply {
            JsonResult::Subscriber(sub) => {
                let subscription = sub.subscriber.subscribe().await;
                loop {
                    // Listen subscription for notifications
                    let notification = subscription.receive().await;

                    // Push notification
                    let j = serde_json::to_string(&notification).unwrap();
                    debug!(target: "rpc::server", "{} <-- {}", peer_addr, j);

                    if let Err(e) = stream.write_all(j.as_bytes()).await {
                        error!(target: "rpc::server", "JSON-RPC server failed writing to {} socket: {}", peer_addr, e);
                        debug!(target: "rpc::server", "Closed connection for {}", peer_addr);
                        break
                    }
                }
                subscription.unsubscribe().await;
            }
            _ => {
                let j = serde_json::to_string(&reply).unwrap();
                debug!(target: "rpc::server", "{} <-- {}", peer_addr, j);

                if let Err(e) = stream.write_all(j.as_bytes()).await {
                    error!(target: "rpc::server", "JSON-RPC server failed writing to {} socket: {}", peer_addr, e);
                    debug!(target: "rpc::server", "Closed connection for {}", peer_addr);
                    break
                }
            }
        }
    }

    Ok(())
}

/// Wrapper function around [`accept()`] to take the incoming connection and
/// pass it forward.
async fn run_accept_loop(
    listener: Box<dyn TransportListener>,
    rh: Arc<impl RequestHandler + 'static>,
    ex: Arc<smol::Executor<'_>>,
) -> Result<()> {
    while let Ok((stream, peer_addr)) = listener.next().await {
        info!(target: "rpc::server", "JSON-RPC server accepted connection from {}", peer_addr);
        // Detaching requests handling
        let _rh = rh.clone();
        ex.spawn(async move {
            if let Err(e) = accept(stream, peer_addr.clone(), _rh).await {
                error!(target: "rpc::server", "JSON-RPC server error on handling request of {}: {}", peer_addr, e);
            }
        }).detach();
    }

    Ok(())
}

/// Start a JSON-RPC server bound to the given accept URL and use the given
/// [`RequestHandler`] to handle incoming requests.
pub async fn listen_and_serve(
    accept_url: Url,
    rh: Arc<impl RequestHandler + 'static>,
    ex: Arc<smol::Executor<'_>>,
) -> Result<()> {
    debug!(target: "rpc::server", "Trying to bind listener on {}", accept_url);

    macro_rules! accept {
        ($listener:expr, $transport:expr, $upgrade:expr) => {{
            if let Err(err) = $listener {
                error!(target: "rpc::server", "JSON-RPC server setup for {} failed: {}", accept_url, err);
                return Err(Error::BindFailed(accept_url.as_str().into()))
            }

            let listener = $listener?.await;
            if let Err(err) = listener {
                error!(target: "rpc::server", "JSON-RPC listener bind to {} failed: {}", accept_url, err);
                return Err(Error::BindFailed(accept_url.as_str().into()))
            }

            let listener = listener?;
            match $upgrade {
                None => {
                    info!(target: "rpc::server", "JSON-RPC listener bound to {}", accept_url);
                    run_accept_loop(Box::new(listener), rh, ex.clone()).await?;
                }
                Some(u) if u == "tls" => {
                    let tls_listener = $transport.upgrade_listener(listener)?.await?;
                    info!(target: "rpc::server", "JSON-RPC listener bound to {}", accept_url);
                    run_accept_loop(Box::new(tls_listener), rh, ex.clone()).await?;
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
            let (socks5_url, torc_url, auth_cookie) = TorTransport::get_listener_env()?;
            let auth_cookie = hex::encode(&std::fs::read(auth_cookie).unwrap());
            let transport = TorTransport::new(socks5_url, Some((torc_url, auth_cookie)))?;

            // Generate EHS pointing to local address
            let hurl = transport.create_ehs(accept_url.clone())?;
            info!(target: "rpc::server", "Created ephemeral hidden service: {}", hurl.to_string());

            let listener = transport.clone().listen_on(accept_url.clone());
            accept!(listener, transport, upgrade);
        }
        TransportName::Unix => {
            let transport = UnixTransport::new();
            let listener = transport.listen_on(accept_url.clone());
            accept!(listener, transport, None);
        }
        _ => unimplemented!(),
    }

    Ok(())
}
