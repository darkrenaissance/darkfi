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

//! JSON-RPC server-side implementation.
use async_std::sync::Arc;
use async_trait::async_trait;
use futures::{AsyncReadExt, AsyncWriteExt};
use log::{debug, error, info, warn};
use serde::Serialize;
use url::Url;

use super::jsonrpc::{JsonRequest, JsonResult};
use crate::{
    net::transport::{
        TcpTransport, TorTransport, Transport, TransportListener, TransportName, TransportStream,
        UnixTransport,
    },
    system::SubscriberPtr,
    Error, Result,
};

/// Asynchronous trait implementing an internal accept function
/// that runs inside a loop for accepting incoming JSON-RPC requests
/// and passing them to the handler trait.
#[async_trait]
pub trait AcceptTrait {
    async fn accept(
        self: Arc<Self>,
        mut stream: Box<dyn TransportStream>,
        peer_addr: Url,
    ) -> Result<()>;
}

/// Asynchronous trait implementing a handler for incoming JSON-RPC requests.
/// Can be used by matching on methods and branching out to functions that
/// handle respective methods.
#[async_trait]
pub trait RequestHandler: Sync + Send {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult;
}

#[async_trait]
impl<T: RequestHandler> AcceptTrait for T {
    async fn accept(
        self: Arc<Self>,
        mut stream: Box<dyn TransportStream>,
        peer_addr: Url,
    ) -> Result<()> {
        loop {
            // FIXME: Nasty size. 8M
            let mut buf = vec![0; 1024 * 8192];

            let n = match stream.read(&mut buf).await {
                Ok(n) if n == 0 => {
                    debug!(target: "jsonrpc-server", "Closed connection for {}", peer_addr);
                    break
                }
                Ok(n) => n,
                Err(e) => {
                    error!("JSON-RPC server failed reading from {} socket: {}", peer_addr, e);
                    debug!(target: "jsonrpc-server", "Closed connection for {}", peer_addr);
                    break
                }
            };

            let r: JsonRequest = match serde_json::from_slice(&buf[0..n]) {
                Ok(r) => {
                    debug!(target: "jsonrpc-server", "{} --> {}", peer_addr, String::from_utf8_lossy(&buf));
                    r
                }
                Err(e) => {
                    warn!("JSON-RPC server received invalid JSON from {}: {}", peer_addr, e);
                    debug!(target: "jsonrpc-server", "Closed connection for {}", peer_addr);
                    break
                }
            };

            let reply = self.handle_request(r).await;
            let j = serde_json::to_string(&reply).unwrap();
            debug!(target: "jsonrpc-server", "{} <-- {}", peer_addr, j);

            if let Err(e) = stream.write_all(j.as_bytes()).await {
                error!("JSON-RPC server failed writing to {} socket: {}", peer_addr, e);
                debug!(target: "jsonrpc-server", "Closed connection for {}", peer_addr);
                break
            }
        }

        Ok(())
    }
}

/// Wrapper struct to notify an incoming connection about subscription items.
pub struct NotifyHandler<T> {
    subscriber: SubscriberPtr<T>,
}

impl<T: Clone + Serialize> NotifyHandler<T> {
    pub async fn new(subscriber: SubscriberPtr<T>) -> Arc<Self> {
        Arc::new(Self { subscriber })
    }
}

#[async_trait]
impl<T: Clone + Send + Serialize> AcceptTrait for NotifyHandler<T> {
    async fn accept(
        self: Arc<Self>,
        mut stream: Box<dyn TransportStream>,
        peer_addr: Url,
    ) -> Result<()> {
        let subscription = self.subscriber.clone().subscribe().await;

        loop {
            // Listen subscription for notifications
            let notification = subscription.receive().await;

            // Push notification
            let j = serde_json::to_string(&notification).unwrap();
            debug!(target: "jsonrpc-server", "{} <-- {}", peer_addr, j);

            if let Err(e) = stream.write_all(j.as_bytes()).await {
                debug!(target: "jsonrpc-server", "JSON-RPC server failed writing to {} socket: {}", peer_addr, e);
                debug!(target: "jsonrpc-server", "Closed connection for {}", peer_addr);
                break
            }
        }

        subscription.unsubscribe().await;

        Ok(())
    }
}

/// Wrapper function around [`accept()`] to take the incoming connection and
/// pass it forward.
async fn run_accept_loop(
    listener: Box<dyn TransportListener>,
    handler: Arc<impl AcceptTrait + 'static>,
) -> Result<()> {
    while let Ok((stream, peer_addr)) = listener.next().await {
        info!("JSON-RPC server accepted connection from {}", peer_addr);
        handler.clone().accept(stream, peer_addr).await?;
    }

    Ok(())
}

/// Start a JSON-RPC server bound to the given accept URL and use the given
/// [`RequestHandler`] to handle incoming requests.
pub async fn listen_and_serve(
    accept_url: Url,
    handler: Arc<impl AcceptTrait + 'static>,
) -> Result<()> {
    debug!(target: "jsonrpc-server", "Trying to bind listener on {}", accept_url);

    macro_rules! accept {
        ($listener:expr, $transport:expr, $upgrade:expr) => {{
            if let Err(err) = $listener {
                error!("JSON-RPC server setup for {} failed: {}", accept_url, err);
                return Err(Error::BindFailed(accept_url.as_str().into()))
            }

            let listener = $listener?.await;
            if let Err(err) = listener {
                error!("JSON-RPC listener bind to {} failed: {}", accept_url, err);
                return Err(Error::BindFailed(accept_url.as_str().into()))
            }

            let listener = listener?;
            match $upgrade {
                None => {
                    info!("JSON-RPC listener bound to {}", accept_url);
                    run_accept_loop(Box::new(listener), handler).await?;
                }
                Some(u) if u == "tls" => {
                    let tls_listener = $transport.upgrade_listener(listener)?.await?;
                    info!("JSON-RPC listener bound to {}", accept_url);
                    run_accept_loop(Box::new(tls_listener), handler).await?;
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
            info!("Created ephemeral hidden service: {}", hurl.to_string());

            let listener = transport.clone().listen_on(accept_url.clone());
            accept!(listener, transport, upgrade);
        }
        TransportName::Unix => {
            let transport = UnixTransport::new();
            let listener = transport.listen(accept_url.clone()).await;
            if let Err(err) = listener {
                error!("JSON-RPC Unix socket bind to {} failed: {}", accept_url, err);
                return Err(Error::BindFailed(accept_url.as_str().into()))
            }
            run_accept_loop(Box::new(listener?), handler).await?;
        }
        _ => unimplemented!(),
    }

    Ok(())
}
