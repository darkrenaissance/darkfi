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

use log::{debug, error};
use smol::{channel, Executor};
use tinyjson::JsonValue;
use url::Url;

use super::{
    common::{read_from_stream, write_to_stream, INIT_BUF_SIZE},
    jsonrpc::*,
};
use crate::{
    net::transport::{Dialer, PtStream},
    system::{StoppableTask, StoppableTaskPtr, SubscriberPtr},
    Error, Result,
};

/// JSON-RPC client implementation using asynchronous channels.
pub struct RpcClient {
    /// The channel used to send JSON-RPC request objects.
    /// The `bool` marks if we should have a reply read timeout.
    req_send: channel::Sender<(JsonRequest, bool)>,
    /// The channel used to read the JSON-RPC response object.
    rep_recv: channel::Receiver<JsonResult>,
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

        // Instantiate Dialer and dial the server
        // TODO: Could add a timeout here
        let dialer = Dialer::new(endpoint).await?;
        let stream = dialer.dial(None).await?;

        // Create the StoppableTask running the request-reply loop.
        // This represents the actual connection, which can be stopped
        // using `RpcClient::stop()`.
        let task = StoppableTask::new();
        task.clone().start(
            Self::reqrep_loop(stream, rep_send, req_recv),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcClientStopped) => {}
                    Err(e) => error!(target: "rpc::client", "[RPC] Client error: {}", e),
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
        stream: Box<dyn PtStream>,
        rep_send: channel::Sender<JsonResult>,
        req_recv: channel::Receiver<(JsonRequest, bool)>,
    ) -> Result<()> {
        debug!(target: "rpc::client::reqrep_loop()", "Starting reqrep loop");

        let (mut reader, mut writer) = smol::io::split(stream);

        loop {
            let mut buf = Vec::with_capacity(INIT_BUF_SIZE);

            let (request, with_timeout) = req_recv.recv().await?;

            let request = JsonResult::Request(request);
            write_to_stream(&mut writer, &request).await?;

            let _ = read_from_stream(&mut reader, &mut buf, with_timeout).await?;
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
    pub async fn subscribe(&self, req: JsonRequest, sub: SubscriberPtr<JsonResult>) -> Result<()> {
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
                    sub.notify(notification.clone()).await;
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
