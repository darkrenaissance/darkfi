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

use async_std::sync::Arc;
use futures::{select, FutureExt};
use log::{debug, error};
use smol::channel::{Receiver, Sender};
use tinyjson::JsonValue;
use url::Url;

use super::{
    common::{read_from_stream, write_to_stream, INIT_BUF_SIZE},
    jsonrpc::*,
};
use crate::{
    net::transport::{Dialer, PtStream},
    system::SubscriberPtr,
    Error, Result,
};

/// JSON-RPC client implementation using asynchronous channels.
pub struct RpcClient {
    sender: Sender<(JsonRequest, bool)>,
    receiver: Receiver<JsonResult>,
    stop_signal: Sender<()>,
    endpoint: Url,
}

impl RpcClient {
    /// Instantiate a new JSON-RPC client that will connect to the given endpoint
    pub async fn new(endpoint: Url, ex: Option<Arc<smol::Executor<'_>>>) -> Result<Self> {
        let (sender, receiver, stop_signal) = Self::open_channels(endpoint.clone(), ex).await?;
        Ok(Self { sender, receiver, stop_signal, endpoint })
    }

    /// Instantiate async channels for a new [`RpcClient`]
    async fn open_channels(
        endpoint: Url,
        ex: Option<Arc<smol::Executor<'_>>>,
    ) -> Result<(Sender<(JsonRequest, bool)>, Receiver<JsonResult>, Sender<()>)> {
        let (data_send, data_recv) = smol::channel::unbounded();
        let (result_send, result_recv) = smol::channel::unbounded();
        let (stop_send, stop_recv) = smol::channel::bounded(1);

        let dialer = Dialer::new(endpoint).await?;
        // TODO: Could add a timeout here:
        let stream = dialer.dial(None).await?;

        // By passing in an executor we can avoid the global executor provided
        // by these crates. Production usage should actually give an exexutor
        // to `RpcClient::new()`.
        if let Some(ex) = ex {
            ex.spawn(Self::reqrep_loop(stream, result_send, data_recv, stop_recv)).detach();
        } else {
            smol::spawn(Self::reqrep_loop(stream, result_send, data_recv, stop_recv)).detach();
        }

        Ok((data_send, result_recv, stop_send))
    }

    /// Close the channels of an instantiated [`RpcClient`]
    pub async fn close(&self) -> Result<()> {
        self.stop_signal.send(()).await?;
        Ok(())
    }

    /// Internal function that loops on a given stream and multiplexes the data.
    async fn reqrep_loop(
        mut stream: Box<dyn PtStream>,
        result_send: Sender<JsonResult>,
        data_recv: Receiver<(JsonRequest, bool)>,
        stop_recv: Receiver<()>,
    ) -> Result<()> {
        loop {
            let mut buf = Vec::with_capacity(INIT_BUF_SIZE);

            select! {
                tuple = data_recv.recv().fuse() => {
                    let (request, with_timeout) = tuple?;
                    let request = JsonResult::Request(request);
                    write_to_stream(&mut stream, &request).await?;

                    let _ = read_from_stream(&mut stream, &mut buf, with_timeout).await?;
                    let val: JsonValue = String::from_utf8(buf)?.parse()?;
                    let rep = JsonResult::try_from_value(&val)?;
                    result_send.send(rep).await?;
                }

                _ = stop_recv.recv().fuse() => break,
            }
        }

        Ok(())
    }

    /// Send a given JSON-RPC request over the instantiated client and
    /// return a possible result. If the response is an error, returns
    /// a `JsonRpcError`.
    pub async fn request(&self, req: JsonRequest) -> Result<JsonValue> {
        let req_id = req.id;
        debug!(target: "rpc::client", "--> {}", req.stringify()?);

        // If the connection is closed, the sender will get an error
        // for sending to a closed channel.
        if let Err(e) = self.sender.send((req, true)).await {
            error!(
                target: "rpc::client", "[RPC] Client unable to send to {}: {}",
                self.endpoint, e,
            );
            return Err(Error::NetworkOperationFailed)
        }

        // If the connection is closed, the receiver will get an error
        // for waiting on a closed channel.
        let reply = self.receiver.recv().await;
        if let Err(e) = reply {
            error!(
                target: "rpc::client", "[RPC] Client unable to recv from {}: {}",
                self.endpoint, e,
            );
            return Err(Error::NetworkOperationFailed)
        }

        match reply.unwrap() {
            JsonResult::Response(rep) => {
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
                self.stop_signal.send(()).await?;
                return Err(e)
            }
        };

        self.stop_signal.send(()).await?;
        Ok(rep)
    }

    /// Listen instantiated client for notifications.
    /// NOTE: Subscriber listeners must perform response handling.
    pub async fn subscribe(&self, req: JsonRequest, sub: SubscriberPtr<JsonResult>) -> Result<()> {
        // Perform initial request.
        let req_id = req.id;
        debug!(target: "rpc::client", "--> {}", req.stringify()?);

        // If the connection is closed, the sender will get an error for
        // sending to a closed channel.
        if let Err(e) = self.sender.send((req, false)).await {
            error!(target: "rpc::client", "[RPC] Client unable to send to {}: {}", self.endpoint, e);
            return Err(Error::NetworkOperationFailed)
        }

        loop {
            // If the connection is closed, the receiver will get an error
            // for waiting on a closed channel.
            let notification = self.receiver.recv().await;
            if let Err(e) = notification {
                error!(target: "rpc::client", "[RPC] Client unable to recv from {}: {}", self.endpoint, e);
                self.stop_signal.send(()).await?;
                break
            }

            // Notify subscribed channels
            let notification = notification.unwrap();
            match notification {
                JsonResult::Notification(ref n) => {
                    debug!(target: "rpc::client", "<-- {}", n.stringify()?);
                    sub.notify(notification.clone()).await;
                }

                JsonResult::Error(e) => {
                    debug!(target: "rpc::client", "<-- {}", e.stringify()?);
                    return Err(Error::JsonRpcError((e.error.code, e.error.message)))
                }

                JsonResult::Response(r) => {
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

        sub.notify(JsonError::new(ErrorCode::InternalError, None, req_id).into()).await;
        Err(Error::NetworkOperationFailed)
    }
}
