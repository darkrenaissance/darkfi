/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use smol::{channel, io::BufReader, Executor};
use tinyjson::JsonValue;
use tracing::{debug, error};
use url::Url;

use super::{
    common::{
        http_read_from_stream_response, http_write_to_stream, read_from_stream, write_to_stream,
        INIT_BUF_SIZE, READ_TIMEOUT,
    },
    jsonrpc::*,
};
use crate::{
    net::transport::{Dialer, PtStream},
    system::{io_timeout, PublisherPtr, StoppableTask, StoppableTaskPtr},
    Error, Result,
};

/// JSON-RPC client implementation using asynchronous channels.
pub struct RpcClient {
    /// The channel used to send JSON-RPC request objects.
    /// The `bool` marks if we should have a reply read timeout.
    req_send: channel::Sender<(JsonRequest, bool)>,
    /// The channel used to read the JSON-RPC response object.
    rep_recv: channel::Receiver<JsonResult>,
    /// The channel used to skip waiting for a JSON-RPC client request
    req_skip_send: channel::Sender<()>,
    /// The stoppable task pointer, used on [`RpcClient::stop()`]
    task: StoppableTaskPtr,
}

impl RpcClient {
    /// Instantiate a new JSON-RPC client that connects to the given endpoint.
    /// The function takes an `Executor` object, which is needed to start the
    /// `StoppableTask` which represents the client-server connection.
    pub async fn new(endpoint: Url, ex: Arc<Executor<'_>>) -> Result<Self> {
        // Instantiate communication channels
        let (req_send, req_recv) = channel::unbounded();
        let (rep_send, rep_recv) = channel::unbounded();
        let (req_skip_send, req_skip_recv) = channel::unbounded();

        // Figure out if we're using HTTP and rewrite the URL accordingly.
        let mut dialer_url = endpoint.clone();
        if endpoint.scheme().starts_with("http+") {
            let scheme = endpoint.scheme().strip_prefix("http+").unwrap();
            let url_str = endpoint.as_str().replace(endpoint.scheme(), scheme);
            dialer_url = url_str.parse()?;
        }
        let use_http = endpoint.scheme().starts_with("http+");

        // Instantiate Dialer and dial the server
        // TODO: Could add a timeout here
        let dialer = Dialer::new(dialer_url, None, None).await?;
        let stream = dialer.dial(None).await?;

        // Create the StoppableTask running the request-reply loop.
        // This represents the actual connection, which can be stopped
        // using `RpcClient::stop()`.
        let task = StoppableTask::new();
        task.clone().start(
            Self::reqrep_loop(use_http, stream, rep_send, req_recv, req_skip_recv),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcClientStopped) => {}
                    Err(e) => error!(target: "rpc::client", "[RPC] Client error: {e}"),
                }
            },
            Error::RpcClientStopped,
            ex.clone(),
        );

        Ok(Self { req_send, rep_recv, task, req_skip_send })
    }

    /// Stop the JSON-RPC client. This will trigger `stop()` on the inner
    /// `StoppableTaskPtr` resulting in stopping the internal reqrep loop
    /// and therefore closing the connection.
    pub async fn stop(&self) {
        self.task.stop().await;
    }

    /// Internal function that loops on a given stream and multiplexes the data
    async fn reqrep_loop(
        use_http: bool,
        stream: Box<dyn PtStream>,
        rep_send: channel::Sender<JsonResult>,
        req_recv: channel::Receiver<(JsonRequest, bool)>,
        req_skip_recv: channel::Receiver<()>,
    ) -> Result<()> {
        debug!(target: "rpc::client::reqrep_loop", "Starting reqrep loop");

        let (reader, mut writer) = smol::io::split(stream);
        let mut reader = BufReader::new(reader);

        loop {
            let mut buf = Vec::with_capacity(INIT_BUF_SIZE);
            let mut with_timeout = false;

            // Read an incoming client request, or skip it if triggered from
            // a JSONRPC notification subscriber
            smol::future::or(
                async {
                    let (request, timeout) = req_recv.recv().await?;
                    with_timeout = timeout;

                    let request = JsonResult::Request(request);
                    if use_http {
                        http_write_to_stream(&mut writer, &request).await?;
                    } else {
                        write_to_stream(&mut writer, &request).await?;
                    }
                    Ok::<(), crate::Error>(())
                },
                async {
                    req_skip_recv.recv().await?;
                    Ok::<(), crate::Error>(())
                },
            )
            .await?;

            if with_timeout {
                if use_http {
                    let _ = io_timeout(
                        READ_TIMEOUT,
                        http_read_from_stream_response(&mut reader, &mut buf),
                    )
                    .await?;
                } else {
                    let _ =
                        io_timeout(READ_TIMEOUT, read_from_stream(&mut reader, &mut buf)).await?;
                }
            } else {
                #[allow(clippy::collapsible_else_if)]
                if use_http {
                    let _ = http_read_from_stream_response(&mut reader, &mut buf).await?;
                } else {
                    let _ = read_from_stream(&mut reader, &mut buf).await?;
                }
            }

            let val: JsonValue = String::from_utf8(buf)?.parse()?;
            let rep = JsonResult::try_from_value(&val)?;
            rep_send.send(rep).await?;
        }
    }

    /// Send a given JSON-RPC request over the instantiated client and
    /// return a possible result. If the response is an error, returns
    /// a `JsonRpcError`.
    pub async fn request(&self, req: JsonRequest) -> Result<JsonValue> {
        let req_id = req.id;
        debug!(target: "rpc::client", "--> {}", req.stringify()?);

        // If the connection is closed, the sender will get an error
        // for sending to a closed channel.
        self.req_send.send((req, true)).await?;

        // If the connection is closed, the receiver will get an error
        // for waiting on a closed channel.
        let reply = self.rep_recv.recv().await?;

        // Handle the response
        match reply {
            JsonResult::Response(rep) | JsonResult::SubscriberWithReply(_, rep) => {
                debug!(target: "rpc::client", "<-- {}", rep.stringify()?);

                // Check if the IDs match
                if req_id != rep.id {
                    let e = JsonError::new(ErrorCode::IdMismatch, None, rep.id);
                    return Err(Error::JsonRpcError((e.error.code, e.error.message)))
                }

                Ok(rep.result)
            }

            JsonResult::Error(e) => {
                debug!(target: "rpc::client", "<-- {}", e.stringify()?);
                Err(Error::JsonRpcError((e.error.code, e.error.message)))
            }

            JsonResult::Notification(n) => {
                debug!(target: "rpc::client", "<-- {}", n.stringify()?);
                let e = JsonError::new(ErrorCode::InvalidReply, None, req_id);
                Err(Error::JsonRpcError((e.error.code, e.error.message)))
            }

            JsonResult::Request(r) => {
                debug!(target: "rpc::client", "<-- {}", r.stringify()?);
                let e = JsonError::new(ErrorCode::InvalidReply, None, req_id);
                Err(Error::JsonRpcError((e.error.code, e.error.message)))
            }

            JsonResult::Subscriber(_) => {
                // When?
                let e = JsonError::new(ErrorCode::InvalidReply, None, req_id);
                Err(Error::JsonRpcError((e.error.code, e.error.message)))
            }
        }
    }

    /// Oneshot send a given JSON-RPC request over the instantiated client
    /// and immediately close the channels upon receiving a reply.
    pub async fn oneshot_request(&self, req: JsonRequest) -> Result<JsonValue> {
        let rep = match self.request(req).await {
            Ok(v) => v,
            Err(e) => {
                self.stop().await;
                return Err(e)
            }
        };

        self.stop().await;
        Ok(rep)
    }

    /// Listen instantiated client for notifications.
    /// NOTE: Subscriber listeners must perform response handling.
    pub async fn subscribe(
        &self,
        req: JsonRequest,
        publisher: PublisherPtr<JsonResult>,
    ) -> Result<()> {
        // Perform initial request
        debug!(target: "rpc::client", "--> {}", req.stringify()?);
        let req_id = req.id;

        // If the connection is closed, the sender will get an error for
        // sending to a closed channel.
        self.req_send.send((req, false)).await?;

        // Now loop and listen to notifications
        loop {
            // If the connection is closed, the receiver will get an error
            // for waiting on a closed channel.
            let notification = self.rep_recv.recv().await?;

            // Handle the response
            match notification {
                JsonResult::Notification(ref n) => {
                    debug!(target: "rpc::client", "<-- {}", n.stringify()?);
                    self.req_skip_send.send(()).await?;
                    publisher.notify(notification.clone()).await;
                    continue
                }

                JsonResult::Error(e) => {
                    debug!(target: "rpc::client", "<-- {}", e.stringify()?);
                    return Err(Error::JsonRpcError((e.error.code, e.error.message)))
                }

                JsonResult::Response(r) | JsonResult::SubscriberWithReply(_, r) => {
                    debug!(target: "rpc::client", "<-- {}", r.stringify()?);
                    let e = JsonError::new(ErrorCode::InvalidReply, None, req_id);
                    return Err(Error::JsonRpcError((e.error.code, e.error.message)))
                }

                JsonResult::Request(r) => {
                    debug!(target: "rpc::client", "<-- {}", r.stringify()?);
                    let e = JsonError::new(ErrorCode::InvalidReply, None, req_id);
                    return Err(Error::JsonRpcError((e.error.code, e.error.message)))
                }

                JsonResult::Subscriber(_) => {
                    // When?
                    let e = JsonError::new(ErrorCode::InvalidReply, None, req_id);
                    return Err(Error::JsonRpcError((e.error.code, e.error.message)))
                }
            }
        }
    }
}

/// Highly experimental JSON-RPC client implementation using asynchronous channels,
/// with each new request canceling waiting for the previous one. All requests are
/// executed without a timeout.
pub struct RpcChadClient {
    /// The channel used to send JSON-RPC request objects
    req_send: channel::Sender<JsonRequest>,
    /// The channel used to read the JSON-RPC response object
    rep_recv: channel::Receiver<JsonResult>,
    /// The stoppable task pointer, used on [`RpcChadClient::stop()`]
    task: StoppableTaskPtr,
}

impl RpcChadClient {
    /// Instantiate a new JSON-RPC client that connects to the given endpoint.
    /// The function takes an `Executor` object, which is needed to start the
    /// `StoppableTask` which represents the client-server connection.
    pub async fn new(endpoint: Url, ex: Arc<Executor<'_>>) -> Result<Self> {
        // Instantiate communication channels
        let (req_send, req_recv) = channel::unbounded();
        let (rep_send, rep_recv) = channel::unbounded();

        // Figure out if we're using HTTP and rewrite the URL accordingly.
        let mut dialer_url = endpoint.clone();
        if endpoint.scheme().starts_with("http+") {
            let scheme = endpoint.scheme().strip_prefix("http+").unwrap();
            let url_str = endpoint.as_str().replace(endpoint.scheme(), scheme);
            dialer_url = url_str.parse()?;
        }
        let use_http = endpoint.scheme().starts_with("http+");

        // Instantiate Dialer and dial the server
        // TODO: Could add a timeout here
        let dialer = Dialer::new(dialer_url, None, None).await?;
        let stream = dialer.dial(None).await?;

        // Create the StoppableTask running the request-reply loop.
        // This represents the actual connection, which can be stopped
        // using `RpcChadClient::stop()`.
        let task = StoppableTask::new();
        task.clone().start(
            Self::reqrep_loop(use_http, stream, rep_send, req_recv),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcClientStopped) => {}
                    Err(e) => error!(target: "rpc::chad_client", "[RPC] Client error: {e}"),
                }
            },
            Error::RpcClientStopped,
            ex.clone(),
        );

        Ok(Self { req_send, rep_recv, task })
    }

    /// Stop the JSON-RPC client. This will trigger `stop()` on the inner
    /// `StoppableTaskPtr` resulting in stopping the internal reqrep loop
    /// and therefore closing the connection.
    pub async fn stop(&self) {
        self.task.stop().await;
    }

    /// Internal function that loops on a given stream and multiplexes the data
    async fn reqrep_loop(
        use_http: bool,
        stream: Box<dyn PtStream>,
        rep_send: channel::Sender<JsonResult>,
        req_recv: channel::Receiver<JsonRequest>,
    ) -> Result<()> {
        debug!(target: "rpc::chad_client::reqrep_loop", "Starting reqrep loop");

        let (reader, mut writer) = smol::io::split(stream);
        let mut reader = BufReader::new(reader);

        loop {
            let mut buf = Vec::with_capacity(INIT_BUF_SIZE);

            // Read an incoming client request, or wait for a response
            smol::future::or(
                async {
                    let request = req_recv.recv().await?;
                    let request = JsonResult::Request(request);
                    if use_http {
                        http_write_to_stream(&mut writer, &request).await?;
                    } else {
                        write_to_stream(&mut writer, &request).await?;
                    }
                    Ok::<(), crate::Error>(())
                },
                async {
                    if use_http {
                        let _ = http_read_from_stream_response(&mut reader, &mut buf).await?;
                    } else {
                        let _ = read_from_stream(&mut reader, &mut buf).await?;
                    }
                    let val: JsonValue = String::from_utf8(buf)?.parse()?;
                    let rep = JsonResult::try_from_value(&val)?;
                    rep_send.send(rep).await?;
                    Ok::<(), crate::Error>(())
                },
            )
            .await?;
        }
    }

    /// Send a given JSON-RPC request over the instantiated client and
    /// return a possible result. If the response is an error, returns
    /// a `JsonRpcError`.
    pub async fn request(&self, req: JsonRequest) -> Result<JsonValue> {
        // Perform request
        let req_id = req.id;
        debug!(target: "rpc::chad_client", "--> {}", req.stringify()?);

        // If the connection is closed, the sender will get an error
        // for sending to a closed channel.
        self.req_send.send(req).await?;

        // Now loop until we receive our response
        loop {
            // If the connection is closed, the receiver will get an error
            // for waiting on a closed channel.
            let reply = self.rep_recv.recv().await?;

            // Handle the response
            match reply {
                JsonResult::Response(rep) | JsonResult::SubscriberWithReply(_, rep) => {
                    debug!(target: "rpc::chad_client", "<-- {}", rep.stringify()?);

                    // Check if the IDs match
                    if req_id != rep.id {
                        debug!(target: "rpc::chad_client", "Skipping response for request {} as its not our latest({req_id})", rep.id);
                        continue
                    }

                    return Ok(rep.result)
                }

                JsonResult::Error(e) => {
                    debug!(target: "rpc::chad_client", "<-- {}", e.stringify()?);

                    // Check if the IDs match
                    if req_id != e.id {
                        debug!(target: "rpc::chad_client", "Skipping response for request {} as its not our latest({req_id})", e.id);
                        continue
                    }

                    return Err(Error::JsonRpcError((e.error.code, e.error.message)))
                }

                JsonResult::Notification(n) => {
                    debug!(target: "rpc::chad_client", "<-- {}", n.stringify()?);
                    let e = JsonError::new(ErrorCode::InvalidReply, None, req_id);
                    return Err(Error::JsonRpcError((e.error.code, e.error.message)))
                }

                JsonResult::Request(r) => {
                    debug!(target: "rpc::chad_client", "<-- {}", r.stringify()?);
                    let e = JsonError::new(ErrorCode::InvalidReply, None, req_id);
                    return Err(Error::JsonRpcError((e.error.code, e.error.message)))
                }

                JsonResult::Subscriber(_) => {
                    // When?
                    let e = JsonError::new(ErrorCode::InvalidReply, None, req_id);
                    return Err(Error::JsonRpcError((e.error.code, e.error.message)))
                }
            }
        }
    }
}
