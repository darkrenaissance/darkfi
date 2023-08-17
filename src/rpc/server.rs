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
use std::time::Duration;

use async_std::{io::timeout, sync::Arc};
use async_trait::async_trait;
use futures::{AsyncReadExt, AsyncWriteExt};
use log::{debug, error, info};
use url::Url;

use super::jsonrpc::{JsonRequest, JsonResult};
use crate::{
    error::RpcError,
    net::transport::{Listener, PtListener, PtStream},
    Result,
};

/// Asynchronous trait implementing a handler for incoming JSON-RPC requests.
/// Can be used by matching on methods and branching out to functions that
/// handle respective methods.
#[async_trait]
pub trait RequestHandler: Sync + Send {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult;
}

const INIT_BUF_SIZE: usize = 1024; // 1K
const MAX_BUF_SIZE: usize = 1024 * 8192; // 8M
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Internal read function called from `accept` that reads from the active stream.
async fn read_from_stream(stream: &mut Box<dyn PtStream>, buf: &mut Vec<u8>) -> Result<usize> {
    debug!(target: "rpc::server", "Reading from stream...");
    let mut total_read = 0;

    while total_read < MAX_BUF_SIZE {
        buf.resize(total_read + INIT_BUF_SIZE, 0);

        match timeout(READ_TIMEOUT, stream.read(&mut buf[total_read..])).await {
            Ok(0) if total_read == 0 => {
                return Err(RpcError::ConnectionClosed("Connection closed".to_string()).into())
            }
            Ok(0) => break, // Finished reading
            Ok(n) => {
                total_read += n;
                if buf[total_read - 1] == b'\n' {
                    break
                }
            }
            Err(e) => return Err(RpcError::IoError(e.kind()).into()),
        }
    }

    // Truncate buffer to actual data size
    buf.truncate(total_read);
    debug!(target: "rpc::server", "Finished reading {} bytes", total_read);
    Ok(total_read)
}

/// Internal accept function that runs inside a loop for accepting incoming
/// JSON-RPC requests and passing them to the [`RequestHandler`].
async fn accept(
    mut stream: Box<dyn PtStream>,
    addr: Url,
    rh: Arc<impl RequestHandler + 'static>,
) -> Result<()> {
    let mut buf = Vec::with_capacity(INIT_BUF_SIZE);
    loop {
        buf.clear();

        let _ = read_from_stream(&mut stream, &mut buf).await?;

        let r: JsonRequest =
            serde_json::from_slice(&buf).map_err(|e| RpcError::InvalidJson(e.to_string()))?;

        debug!(target: "rpc::server", "{} --> {}", addr, String::from_utf8_lossy(&buf));

        let reply = rh.handle_request(r).await;

        match reply {
            JsonResult::Subscriber(sub) => {
                // Subscribe to the inner method subscriber
                let subscription = sub.subscriber.subscriber.subscribe().await;
                loop {
                    // Listen subscription for notifications
                    let notification = subscription.receive().await;

                    // Push notification
                    let j = serde_json::to_string(&notification).unwrap();
                    debug!(target: "rpc::server", "{} <-- {}", addr, j);

                    if let Err(e) = stream.write_all(j.as_bytes()).await {
                        error!(target: "rpc::server", "[RPC] Server failed writing to {} socket: {}", addr, e);
                        debug!(target: "rpc::server", "Closed connection for {}", addr);
                        break
                    }

                    if let Err(e) = stream.write_all(&[b'\n']).await {
                        error!(target: "rpc::server", "[RPC] Server failed writing to {} socket: {}", addr, e);
                        debug!(target: "rpc::server", "Closed connection for {}", addr);
                        break
                    }
                }
                subscription.unsubscribe().await;
            }
            _ => {
                let j = serde_json::to_string(&reply)
                    .map_err(|e| RpcError::InvalidJson(e.to_string()))?;

                debug!(target: "rpc::server", "{} <-- {}", addr, j);

                if let Err(e) = stream.write_all(j.as_bytes()).await {
                    error!(
                        target: "rpc::server", "[RPC] Server failed writing to {} socket: {}",
                        addr, e
                    );
                    return close_conn(
                        &addr,
                        RpcError::ConnectionClosed("Socket write error".to_string()),
                    )
                }

                if let Err(e) = stream.write_all(&[b'\n']).await {
                    error!(
                        target: "rpc::server", "[RPC] Server failed writing to {} socket: {}",
                        addr, e
                    );
                    return close_conn(
                        &addr,
                        RpcError::ConnectionClosed("Socket write error".to_string()),
                    )
                }
            }
        }
    }
}

/// Helper function for connection closing
fn close_conn(peer_addr: &Url, reason: RpcError) -> Result<()> {
    debug!(target: "rpc::server", "Closed connection for {}", peer_addr);
    Err(reason.into())
}

/// Wrapper function around [`accept()`] to take the incoming connection and
/// pass it forward.
async fn run_accept_loop(
    listener: Box<dyn PtListener>,
    rh: Arc<impl RequestHandler + 'static>,
    ex: Arc<smol::Executor<'_>>,
) -> Result<()> {
    while let Ok((stream, peer_addr)) = listener.next().await {
        info!(target: "rpc::server", "[RPC] Server accepted connection from {}", peer_addr);
        // Detaching requests handling
        let _rh = rh.clone();
        ex.spawn(async move {
            if let Err(e) = accept(stream, peer_addr.clone(), _rh).await {
                error!(target: "rpc::server", "[RPC] Server error on handling request of {}: {}", peer_addr, e);
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

    let listener = Listener::new(accept_url).await?.listen().await?;
    run_accept_loop(listener, rh, ex.clone()).await?;

    Ok(())
}
