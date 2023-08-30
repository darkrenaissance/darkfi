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

use std::{collections::HashSet, io::ErrorKind, sync::Arc};

use async_trait::async_trait;
use log::{debug, error, info};
use smol::lock::MutexGuard;
use tinyjson::JsonValue;
use url::Url;

use super::{
    common::{read_from_stream, write_to_stream, INIT_BUF_SIZE},
    jsonrpc::*,
};
use crate::{
    net::transport::{Listener, PtListener, PtStream},
    system::{StoppableTask, StoppableTaskPtr},
    Error, Result,
};

/// Asynchronous trait implementing a handler for incoming JSON-RPC requests.
#[async_trait]
pub trait RequestHandler: Sync + Send {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult;

    async fn pong(&self, id: u16, _params: JsonValue) -> JsonResult {
        JsonResponse::new(JsonValue::String("pong".to_string()), id).into()
    }

    async fn get_connections(&self) -> MutexGuard<'_, HashSet<StoppableTaskPtr>>;

    async fn mark_connection(&self, task: StoppableTaskPtr) {
        self.get_connections().await.insert(task);
    }

    async fn unmark_connection(&self, task: StoppableTaskPtr) {
        self.get_connections().await.remove(&task);
    }

    async fn active_connections(&self) -> usize {
        self.get_connections().await.len()
    }
}

/// Accept function that should run inside a loop for accepting incoming
/// JSON-RPC requests and passing them to the [`RequestHandler`].
pub async fn accept(
    mut stream: Box<dyn PtStream>,
    addr: Url,
    rh: Arc<impl RequestHandler + 'static>,
    conn_limit: Option<usize>,
) -> Result<()> {
    // If there's a connection limit set, we will refuse connections
    // after this point.
    if let Some(conn_limit) = conn_limit {
        if rh.clone().active_connections().await >= conn_limit {
            debug!(
                target: "rpc::server::accept()",
                "Connection limit reached, refusing new conn"
            );
            return Err(Error::RpcConnectionsExhausted)
        }
    }

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
    conn_limit: Option<usize>,
    ex: Arc<smol::Executor<'_>>,
) -> Result<()> {
    loop {
        match listener.next().await {
            Ok((stream, url)) => {
                let rh_ = rh.clone();
                info!(target: "rpc::server", "[RPC] Server accepted conn from {}", url);
                let task = StoppableTask::new();
                let task_ = task.clone();
                task.clone().start(
                    accept(stream, url.clone(), rh.clone(), conn_limit),
                    |_| async move {
                        rh_.clone().unmark_connection(task_.clone()).await;
                    },
                    Error::ChannelStopped,
                    ex.clone(),
                );

                rh.clone().mark_connection(task.clone()).await;
            }

            // As per accept(2) recommendation:
            Err(e) if e.raw_os_error().is_some() => match e.raw_os_error().unwrap() {
                libc::EAGAIN | libc::ECONNABORTED | libc::EPROTO | libc::EINTR => continue,
                _ => {
                    error!(
                        target: "rpc::server::run_accept_loop()",
                        "[RPC] Server failed listening: {}", e,
                    );
                    error!(
                        target: "rpc::server::run_accept_loop()",
                        "[RPC] Closing accept loop"
                    );
                    return Err(e.into())
                }
            },

            // In case a TLS handshake fails, we'll get this:
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => continue,

            // Errors we didn't handle above:
            Err(e) => {
                error!(
                    target: "rpc::server::run_accept_loop()",
                    "[RPC] Unhandled listener.next() error: {}", e,
                );
                error!(
                    target: "rpc::server::run_accept_loop()",
                    "[RPC] Closing acceptloop"
                );
                return Err(e.into())
            }
        }
    }
}

/// Start a JSON-RPC server bound to the given accept URL and use the
/// given [`RequestHandler`] to handle incoming requests.
pub async fn listen_and_serve(
    accept_url: Url,
    rh: Arc<impl RequestHandler + 'static>,
    conn_limit: Option<usize>,
    ex: Arc<smol::Executor<'_>>,
) -> Result<()> {
    let listener = Listener::new(accept_url).await?.listen().await?;
    run_accept_loop(listener, rh, conn_limit, ex.clone()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{rpc::client::RpcClient, system::msleep};
    use smol::{lock::Mutex, net::TcpListener, Executor};

    struct RpcServer {
        rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    }

    #[async_trait]
    impl RequestHandler for RpcServer {
        async fn handle_request(&self, req: JsonRequest) -> JsonResult {
            match req.method.as_str() {
                "ping" => return self.pong(req.id, req.params).await,
                _ => panic!(),
            }
        }

        async fn get_connections(&self) -> MutexGuard<'_, HashSet<StoppableTaskPtr>> {
            self.rpc_connections.lock().await
        }
    }

    #[test]
    fn conn_manager() -> Result<()> {
        let executor = Arc::new(Executor::new());

        // This simulates a server and a client. Through the function, there
        // are some calls to sleep(), which are used for the tests, because
        // otherwise they execute too fast. In practice, The RPC server is
        // a long-running task so when polled, it should handle things in a
        // correct manner.
        smol::block_on(executor.run(async {
            // Find an available port
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let sockaddr = listener.local_addr()?;
            let endpoint = Url::parse(&format!("tcp://127.0.0.1:{}", sockaddr.port()))?;
            drop(listener);

            let rpc_server = Arc::new(RpcServer { rpc_connections: Mutex::new(HashSet::new()) });

            let server_task = StoppableTask::new();
            server_task.clone().start(
                listen_and_serve(endpoint.clone(), rpc_server.clone(), None, executor.clone()),
                |res| async move {
                    match res {
                        Ok(()) | Err(Error::RpcServerStopped) => {}
                        Err(e) => panic!("{}", e),
                    }
                },
                Error::RpcServerStopped,
                executor.clone(),
            );

            // Let the server spawn
            msleep(500).await;

            // Connect a client
            let rpc_client0 = RpcClient::new(endpoint.clone(), executor.clone()).await?;
            msleep(500).await;
            assert!(rpc_server.active_connections().await == 1);

            // Connect another client
            let rpc_client1 = RpcClient::new(endpoint.clone(), executor.clone()).await?;
            msleep(500).await;
            assert!(rpc_server.active_connections().await == 2);

            // Close the first client
            rpc_client0.close().await?;
            msleep(500).await;
            assert!(rpc_server.active_connections().await == 1);

            // Close the second client
            rpc_client1.close().await?;
            msleep(500).await;
            assert!(rpc_server.active_connections().await == 0);

            // The Listener should be stopped when we stop the server task.
            server_task.stop().await;
            assert!(RpcClient::new(endpoint, executor.clone()).await.is_err());

            Ok(())
        }))
    }
}
