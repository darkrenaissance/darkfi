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

use std::sync::Arc;

use async_trait::async_trait;
use log::{debug, error, info};
use tinyjson::JsonValue;
use url::Url;

use super::{
    common::{read_from_stream, write_to_stream, INIT_BUF_SIZE},
    jsonrpc::*,
};
use crate::{
    net::transport::{Listener, PtListener, PtStream},
    Result,
};

/// Asynchronous trait implementing a handler for incoming JSON-RPC requests.
#[async_trait]
pub trait RequestHandler: Sync + Send {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult;

    async fn pong(&self, id: u16, _params: JsonValue) -> JsonResult {
        JsonResponse::new(JsonValue::String("pong".to_string()), id).into()
    }
}

/// Accept function that should run inside a loop for accepting incoming
/// JSON-RPC requests and passing them to the [`RequestHandler`].
pub async fn accept(
    mut stream: Box<dyn PtStream>,
    addr: Url,
    rh: Arc<impl RequestHandler + 'static>,
) -> Result<()> {
    loop {
        let mut buf = Vec::with_capacity(INIT_BUF_SIZE);
        let _ = read_from_stream(&mut stream, &mut buf, false).await?;
        let val: JsonValue = String::from_utf8(buf)?.parse()?;
        let req = JsonRequest::try_from(&val)?;

        debug!(target: "rpc::server", "{} --> {}", addr, val.stringify()?);

        let rep = rh.handle_request(req).await;

        match rep {
            JsonResult::Subscriber(subscriber) => {
                // Subscribe to the inner method subscriber
                let subscription = subscriber.sub.subscribe().await;
                loop {
                    // Listen for notifications
                    let notification = subscription.receive().await;

                    // Push notification
                    debug!(target: "rpc::server", "{} <-- {}", addr, notification.stringify()?);
                    let notification = JsonResult::Notification(notification);
                    if let Err(e) = write_to_stream(&mut stream, &notification).await {
                        subscription.unsubscribe().await;
                        return Err(e)
                    }
                }
            }

            JsonResult::Request(_) | JsonResult::Notification(_) => {
                unreachable!("Should never happen")
            }

            JsonResult::Response(ref v) => {
                debug!(target: "rpc::server", "{} <-- {}", addr, v.stringify()?);
                write_to_stream(&mut stream, &rep).await?;
            }

            JsonResult::Error(ref v) => {
                debug!(target: "rpc::server", "{} <-- {}", addr, v.stringify()?);
                write_to_stream(&mut stream, &rep).await?;
            }
        }
    }
}

/// Wrapper function around [`accept()`] to take the incoming connection and
/// pass it forward.
async fn run_accept_loop(
    listener: Box<dyn PtListener>,
    rh: Arc<impl RequestHandler + 'static>,
    ex: Arc<smol::Executor<'_>>,
) -> Result<()> {
    while let Ok((stream, peer_addr)) = listener.next().await {
        info!(target: "rpc::server", "[RPC] Server accepted conn from {}", peer_addr);
        // Detaching requests handling
        let rh_ = rh.clone();
        ex.spawn(async move {
            if let Err(e) = accept(stream, peer_addr.clone(), rh_).await {
                if e.to_string().as_str() == "Connection closed: Connection closed cleanly" {
                    info!(
                        target: "rpc::server",
                        "[RPC] Closed connection from {}",
                        peer_addr,
                    );
                } else {
                    error!(
                        target: "rpc::server",
                        "[RPC] Server error on handling request from {}: {}",
                        peer_addr, e,
                    );
                }
            }
        })
        .detach();
    }

    // NOTE: This is here now to catch some code path. Will be handled properly.
    panic!("RPC server listener stopped/crashed");
}

/// Start a JSON-RPC server bound to the given accept URL and use the
/// given [`RequestHandler`] to handle incoming requests.
pub async fn listen_and_serve(
    accept_url: Url,
    rh: Arc<impl RequestHandler + 'static>,
    ex: Arc<smol::Executor<'_>>,
) -> Result<()> {
    let listener = Listener::new(accept_url).await?.listen().await?;
    run_accept_loop(listener, rh, ex.clone()).await
}
