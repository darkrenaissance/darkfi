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

use std::{collections::HashSet, io::ErrorKind, sync::Arc};

use async_trait::async_trait;
use smol::{
    io::{BufReader, ReadHalf, WriteHalf},
    lock::{Mutex, MutexGuard},
};
use tinyjson::JsonValue;
use tracing::{debug, info, warn};
use url::Url;

use super::{
    common::{
        http_read_from_stream_request, http_write_to_stream, read_from_stream, write_to_stream,
        INIT_BUF_SIZE,
    },
    jsonrpc::*,
    settings::RpcSettings,
};
use crate::{
    net::transport::{Listener, PtListener, PtStream},
    system::{StoppableTask, StoppableTaskPtr},
    util::logger::verbose,
    Error, Result,
};

/// Asynchronous trait implementing a handler for incoming JSON-RPC requests.
#[async_trait]
pub trait RequestHandler<T>: Sync + Send {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult;

    async fn pong(&self, id: u16, _params: JsonValue) -> JsonResult {
        JsonResponse::new(JsonValue::String("pong".to_string()), id).into()
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>>;

    async fn connections(&self) -> Vec<StoppableTaskPtr> {
        self.connections_mut().await.iter().cloned().collect()
    }

    async fn mark_connection(&self, task: StoppableTaskPtr) {
        self.connections_mut().await.insert(task);
    }

    async fn unmark_connection(&self, task: StoppableTaskPtr) {
        self.connections_mut().await.remove(&task);
    }

    async fn active_connections(&self) -> usize {
        self.connections_mut().await.len()
    }

    async fn stop_connections(&self) {
        info!(target: "rpc::server", "[RPC] Server stopped, closing connections");
        for (i, task) in self.connections().await.iter().enumerate() {
            debug!(target: "rpc::server", "Stopping connection #{i}");
            task.stop().await;
        }
    }
}

/// Auxiliary function to handle a request in the background.
async fn handle_request<T>(
    writer: Arc<Mutex<WriteHalf<Box<dyn PtStream>>>>,
    addr: Url,
    rh: Arc<impl RequestHandler<T> + 'static>,
    ex: Arc<smol::Executor<'_>>,
    tasks: Arc<Mutex<HashSet<Arc<StoppableTask>>>>,
    settings: RpcSettings,
    req: JsonRequest,
) -> Result<()> {
    // Handle disabled RPC methods
    let rep = if settings.is_method_disabled(&req.method) {
        debug!(target: "rpc::server", "RPC method {} is disabled", req.method);
        JsonError::new(ErrorCode::MethodNotFound, None, req.id).into()
    } else {
        rh.handle_request(req).await
    };

    match rep {
        JsonResult::Subscriber(subscriber) => {
            let task = StoppableTask::new();

            // Clone what needs to go in the background
            let task_ = task.clone();
            let addr_ = addr.clone();
            let tasks_ = tasks.clone();
            let writer_ = writer.clone();

            // Detach the subscriber so we can multiplex further requests
            task.clone().start(
                async move {
                    // Subscribe to the inner method subscriber
                    let subscription = subscriber.publisher.subscribe().await;
                    loop {
                        // Listen for notifications
                        let notification = subscription.receive().await;

                        // Push notification
                        debug!(target: "rpc::server", "{addr_} <-- {}", notification.stringify().unwrap());
                        let notification = JsonResult::Notification(notification);

                        let mut writer_lock = writer_.lock().await;

                        #[allow(clippy::collapsible_else_if)]
                        if settings.use_http() {
                            if let Err(e) = http_write_to_stream(&mut writer_lock, &notification).await {
                                subscription.unsubscribe().await;
                                return Err(e.into())
                            }
                        } else {
                            if let Err(e) = write_to_stream(&mut writer_lock, &notification).await {
                                subscription.unsubscribe().await;
                                return Err(e.into())
                            }
                        }

                        drop(writer_lock);
                    }
                },
                move |_| async move {
                    debug!(
                        target: "rpc::server",
                        "Removing background task {} from map", task_.task_id,
                    );
                    tasks_.lock().await.remove(&task_);
                },
                Error::DetachedTaskStopped,
                ex.clone(),
            );

            debug!(target: "rpc::server", "Adding background task {} to map", task.task_id);
            tasks.lock().await.insert(task);
        }

        JsonResult::SubscriberWithReply(subscriber, reply) => {
            // Write the response
            debug!(target: "rpc::server", "{addr} <-- {}", reply.stringify()?);
            let mut writer_lock = writer.lock().await;
            if settings.use_http() {
                http_write_to_stream(&mut writer_lock, &reply.into()).await?;
            } else {
                write_to_stream(&mut writer_lock, &reply.into()).await?;
            }
            drop(writer_lock);

            let task = StoppableTask::new();
            // Clone what needs to go in the background
            let task_ = task.clone();
            let addr_ = addr.clone();
            let tasks_ = tasks.clone();
            let writer_ = writer.clone();

            // Detach the subscriber so we can multiplex further requests
            task.clone().start(
                async move {
                    // Start the subscriber loop
                    let subscription = subscriber.publisher.subscribe().await;
                    loop {
                        // Listen for notifications
                        let notification = subscription.receive().await;

                        // Push notification
                        debug!(target: "rpc::server", "{addr_} <-- {}", notification.stringify().unwrap());
                        let notification = JsonResult::Notification(notification);

                        let mut writer_lock = writer_.lock().await;
                        #[allow(clippy::collapsible_else_if)]
                        if settings.use_http() {
                            if let Err(e) = http_write_to_stream(&mut writer_lock, &notification).await {
                                subscription.unsubscribe().await;
                                drop(writer_lock);
                                return Err(e.into())
                            }
                        } else {
                            if let Err(e) = write_to_stream(&mut writer_lock, &notification).await {
                                subscription.unsubscribe().await;
                                drop(writer_lock);
                                return Err(e.into())
                            }
                        }
                        drop(writer_lock);
                    }
                },
                move |_| async move {
                    debug!(
                        target: "rpc::server",
                        "Removing background task {} from map", task_.task_id,
                    );
                    tasks_.lock().await.remove(&task_);
                },
                Error::DetachedTaskStopped,
                ex.clone(),
            );

            debug!(target: "rpc::server", "Adding background task {} to map", task.task_id);
            tasks.lock().await.insert(task);
        }

        JsonResult::Request(_) | JsonResult::Notification(_) => {
            unreachable!("Should never happen")
        }

        JsonResult::Response(ref v) => {
            debug!(target: "rpc::server", "{addr} <-- {}", v.stringify()?);
            let mut writer_lock = writer.lock().await;
            if settings.use_http() {
                http_write_to_stream(&mut writer_lock, &rep).await?;
            } else {
                write_to_stream(&mut writer_lock, &rep).await?;
            }
            drop(writer_lock);
        }

        JsonResult::Error(ref v) => {
            debug!(target: "rpc::server", "{addr} <-- {}", v.stringify()?);
            let mut writer_lock = writer.lock().await;
            if settings.use_http() {
                http_write_to_stream(&mut writer_lock, &rep).await?;
            } else {
                write_to_stream(&mut writer_lock, &rep).await?;
            }
            drop(writer_lock);
        }
    }

    Ok(())
}

/// Accept function that should run inside a loop for accepting incoming
/// JSON-RPC requests and passing them to the [`RequestHandler`].
#[allow(clippy::type_complexity)]
pub async fn accept<'a, T: 'a>(
    reader: Arc<Mutex<BufReader<ReadHalf<Box<dyn PtStream>>>>>,
    writer: Arc<Mutex<WriteHalf<Box<dyn PtStream>>>>,
    addr: Url,
    rh: Arc<impl RequestHandler<T> + 'static>,
    conn_limit: Option<usize>,
    settings: RpcSettings,
    ex: Arc<smol::Executor<'a>>,
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

    // We'll hold our background tasks here
    let tasks = Arc::new(Mutex::new(HashSet::new()));

    loop {
        let mut buf = Vec::with_capacity(INIT_BUF_SIZE);

        let mut reader_lock = reader.lock().await;
        if settings.use_http() {
            let _ = http_read_from_stream_request(&mut reader_lock, &mut buf).await?;
        } else {
            let _ = read_from_stream(&mut reader_lock, &mut buf).await?;
        }
        drop(reader_lock);

        let line = match String::from_utf8(buf) {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    target: "rpc::server::accept()",
                    "[RPC SERVER] Failed parsing string from read buffer: {e}"
                );
                return Err(e.into())
            }
        };

        // Parse the line as JSON
        let val: JsonValue = match line.trim().parse() {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    target: "rpc::server::accept()",
                    "[RPC SERVER] Failed parsing JSON string: {e}"
                );
                return Err(e.into())
            }
        };

        // Cast to JsonRequest
        let req = match JsonRequest::try_from(&val) {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    target: "rpc::server::accept()",
                    "[RPC SERVER] Failed casting JSON to a JsonRequest: {e}"
                );
                return Err(e.into())
            }
        };

        debug!(target: "rpc::server", "{addr} --> {}", val.stringify()?);

        // Create a new task to handle request in the background
        let task = StoppableTask::new();

        // Clone what needs to go in the background
        let task_ = task.clone();
        let tasks_ = tasks.clone();

        // Detach the task
        task.clone().start(
            handle_request(
                writer.clone(),
                addr.clone(),
                rh.clone(),
                ex.clone(),
                tasks.clone(),
                settings.clone(),
                req,
            ),
            move |_| async move {
                debug!(
                    target: "rpc::server",
                    "Removing background task {} from map", task_.task_id,
                );
                tasks_.lock().await.remove(&task_);
            },
            Error::DetachedTaskStopped,
            ex.clone(),
        );

        debug!(target: "rpc::server", "Adding background task {} to map", task.task_id);
        tasks.lock().await.insert(task);
    }
}

/// Wrapper function around [`accept()`] to take the incoming connection and
/// pass it forward.
async fn run_accept_loop<'a, T: 'a>(
    listener: Box<dyn PtListener>,
    rh: Arc<impl RequestHandler<T> + 'static>,
    conn_limit: Option<usize>,
    settings: RpcSettings,
    ex: Arc<smol::Executor<'a>>,
) -> Result<()> {
    loop {
        match listener.next().await {
            Ok((stream, url)) => {
                let rh_ = rh.clone();
                verbose!(target: "rpc::server", "[RPC] Server accepted conn from {url}");

                let (reader, writer) = smol::io::split(stream);
                let reader = Arc::new(Mutex::new(BufReader::new(reader)));
                let writer = Arc::new(Mutex::new(writer));

                let task = StoppableTask::new();
                let task_ = task.clone();
                let ex_ = ex.clone();
                task.clone().start(
                    accept(
                        reader,
                        writer,
                        url.clone(),
                        rh.clone(),
                        conn_limit,
                        settings.clone(),
                        ex_,
                    ),
                    |_| async move {
                        verbose!(target: "rpc::server", "[RPC] Closed conn from {url}");
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
                libc::ECONNRESET => {
                    warn!(
                        target: "rpc::server::run_accept_loop()",
                        "[RPC] Connection reset by peer in accept_loop"
                    );
                    continue
                }
                libc::ETIMEDOUT => {
                    warn!(
                        target: "rpc::server::run_accept_loop()",
                        "[RPC] Connection timed out in accept_loop"
                    );
                    continue
                }
                libc::EPIPE => {
                    warn!(
                        target: "rpc::server::run_accept_loop()",
                        "[RPC] Broken pipe in accept_loop"
                    );
                    continue
                }
                x => {
                    warn!(
                        target: "rpc::server::run_accept_loop()",
                        "[RPC] Unhandled OS Error: {e} {x}"
                    );
                    continue
                }
            },

            // In case a TLS handshake fails, we'll get this:
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => continue,

            // Handle ErrorKind::Other
            Err(e) if e.kind() == ErrorKind::Other => {
                if let Some(inner) = std::error::Error::source(&e) {
                    if let Some(inner) = inner.downcast_ref::<futures_rustls::rustls::Error>() {
                        warn!(
                            target: "rpc::server::run_accept_loop()",
                            "[RPC] rustls listener error: {inner:?}"
                        );
                        continue
                    }
                }

                warn!(
                    target: "rpc::server::run_accept_loop()",
                    "[RPC] Unhandled ErrorKind::Other error: {e:?}"
                );
                continue
            }

            // Errors we didn't handle above:
            Err(e) => {
                warn!(
                    target: "rpc::server::run_accept_loop()",
                    "[RPC] Unhandled listener.next() error: {e}"
                );
                continue
            }
        }
    }
}

/// Start a JSON-RPC server bound to the givven accept URL and use the
/// given [`RequestHandler`] to handle incoming requests.
///
/// The supported network schemes can be prefixed with `http+` to serve
/// JSON-RPC over HTTP/1.1.
pub async fn listen_and_serve<'a, T: 'a>(
    settings: RpcSettings,
    rh: Arc<impl RequestHandler<T> + 'static>,
    conn_limit: Option<usize>,
    ex: Arc<smol::Executor<'a>>,
) -> Result<()> {
    // Figure out if we're using HTTP and rewrite the URL accordingly.
    let mut listen_url = settings.listen.clone();
    if settings.listen.scheme().starts_with("http+") {
        let scheme = settings.listen.scheme().strip_prefix("http+").unwrap();
        let url_str = settings.listen.as_str().replace(settings.listen.scheme(), scheme);
        listen_url = url_str.parse()?;
    }

    let listener = Listener::new(listen_url, None).await?.listen().await?;

    run_accept_loop(listener, rh, conn_limit, settings, ex.clone()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{rpc::client::RpcClient, system::msleep};
    use smol::{net::TcpListener, Executor};

    struct RpcServer {
        rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    }

    #[async_trait]
    impl RequestHandler<()> for RpcServer {
        async fn handle_request(&self, req: JsonRequest) -> JsonResult {
            match req.method.as_str() {
                "ping" => return self.pong(req.id, req.params).await,
                _ => panic!(),
            }
        }

        async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
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
            let settings = RpcSettings {
                listen: Url::parse(&format!("tcp://127.0.0.1:{}", sockaddr.port()))?,
                disabled_methods: vec![],
            };
            drop(listener);

            let rpc_server = Arc::new(RpcServer { rpc_connections: Mutex::new(HashSet::new()) });
            let rpc_server_ = rpc_server.clone();

            let server_task = StoppableTask::new();
            server_task.clone().start(
                listen_and_serve(settings.clone(), rpc_server.clone(), None, executor.clone()),
                |res| async move {
                    match res {
                        Ok(()) | Err(Error::RpcServerStopped) => {
                            rpc_server_.stop_connections().await
                        }
                        Err(e) => panic!("{e}"),
                    }
                },
                Error::RpcServerStopped,
                executor.clone(),
            );

            // Let the server spawn
            msleep(500).await;

            // Connect a client
            let rpc_client0 = RpcClient::new(settings.listen.clone(), executor.clone()).await?;
            msleep(500).await;
            assert!(rpc_server.active_connections().await == 1);

            // Connect another client
            let rpc_client1 = RpcClient::new(settings.listen.clone(), executor.clone()).await?;
            msleep(500).await;
            assert!(rpc_server.active_connections().await == 2);

            // And another one
            let _rpc_client2 = RpcClient::new(settings.listen.clone(), executor.clone()).await?;
            msleep(500).await;
            assert!(rpc_server.active_connections().await == 3);

            // Close the first client
            rpc_client0.stop().await;
            msleep(500).await;
            assert!(rpc_server.active_connections().await == 2);

            // Close the second client
            rpc_client1.stop().await;
            msleep(500).await;
            assert!(rpc_server.active_connections().await == 1);

            // The Listener should be stopped when we stop the server task.
            server_task.stop().await;
            assert!(RpcClient::new(settings.listen, executor.clone()).await.is_err());

            // After the server is stopped, the connections tasks should also be stopped
            assert!(rpc_server.active_connections().await == 0);

            Ok(())
        }))
    }
}
