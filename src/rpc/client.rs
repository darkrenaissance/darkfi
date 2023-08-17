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

//! JSON-RPC client-side implementation.
use std::time::Duration;

use async_std::{
    io::{timeout, ReadExt, WriteExt},
    sync::Arc,
};
use futures::{select, FutureExt};
use log::{debug, error};
use serde_json::{json, Value};
use url::Url;

use super::jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult};
use crate::{
    error::RpcError,
    net::transport::{Dialer, PtStream},
    Error, Result,
};

const INIT_BUF_SIZE: usize = 1024; // 1K
const MAX_BUF_SIZE: usize = 1024 * 8192; // 1M
const READ_TIMEOUT: Duration = Duration::from_secs(60);

/// JSON-RPC client implementation using asynchronous channels.
pub struct RpcClient {
    send: smol::channel::Sender<(Value, bool)>,
    recv: smol::channel::Receiver<JsonResult>,
    stop_signal: smol::channel::Sender<()>,
    endpoint: Url,
}

impl RpcClient {
    /// Instantiate a new JSON-RPC client that will connect to the given endpoint
    pub async fn new(endpoint: Url, executor: Option<Arc<smol::Executor<'_>>>) -> Result<Self> {
        let (send, recv, stop_signal) = Self::open_channels(&endpoint, executor.clone()).await?;
        Ok(Self { send, recv, stop_signal, endpoint })
    }

    /// Instantiate channels for a new [`RpcClient`]
    async fn open_channels(
        endpoint: &Url,
        executor: Option<Arc<smol::Executor<'_>>>,
    ) -> Result<(
        smol::channel::Sender<(Value, bool)>,
        smol::channel::Receiver<JsonResult>,
        smol::channel::Sender<()>,
    )> {
        let (data_send, data_recv) = smol::channel::unbounded();
        let (result_send, result_recv) = smol::channel::unbounded();
        let (stop_send, stop_recv) = smol::channel::unbounded();

        let dialer = Dialer::new(endpoint.clone()).await?;
        let stream = dialer.dial(None).await?;

        if let Some(ex) = executor {
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

    /// Internal read function called from `reqrep_loop` that reads from the
    /// active stream.
    async fn read_from_stream(stream: &mut Box<dyn PtStream>, buf: &mut Vec<u8>) -> Result<usize> {
        debug!(target: "rpc::client", "Reading from stream...");
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
        debug!(target: "rpc::client", "Finished reading {} bytes", total_read);
        Ok(total_read)
    }

    /// Internal function that loops on a given stream and multiplexes the data.
    async fn reqrep_loop(
        mut stream: Box<dyn PtStream>,
        result_send: smol::channel::Sender<JsonResult>,
        data_recv: smol::channel::Receiver<(Value, bool)>,
        stop_recv: smol::channel::Receiver<()>,
    ) -> Result<()> {
        let mut buf = Vec::with_capacity(INIT_BUF_SIZE);

        loop {
            buf.clear();

            select! {
                tuple = data_recv.recv().fuse() => {
                    let (data, _with_timeout) = tuple?;
                    let data_bytes = serde_json::to_vec(&data)?;
                    stream.write_all(&data_bytes).await?;
                    stream.write_all(&[b'\n']).await?;

                    let _ = Self::read_from_stream(&mut stream, &mut buf).await?;

                    let r: JsonResult = serde_json::from_slice(&buf).map_err(
                        |e| RpcError::InvalidJson(e.to_string())
                    )?;

                    result_send.send(r).await?;
                }

                _ = stop_recv.recv().fuse() => break
            }
        }

        Ok(())
    }

    /// Send a given JSON-RPC request over the instantiated client.
    pub async fn request(&self, value: JsonRequest) -> Result<Value> {
        let req_id = value.id.clone().as_u64().unwrap();

        debug!(target: "rpc::client", "--> {}", serde_json::to_string(&value)?);

        // If the connection is closed, the sender will get an error
        // for sending to a closed channel.
        if let Err(e) = self.send.send((json!(value), true)).await {
            error!(
                target: "rpc::client", "[RPC] Client unable to send to {}: {}",
                self.endpoint, e
            );
            return Err(Error::NetworkOperationFailed)
        }

        // If the connection is closed, the receiver will get an error
        // for waiting on a closed channel.
        let reply = self.recv.recv().await;
        if let Err(e) = reply {
            error!(
                target: "rpc::client", "[RPC] Client unable to recv from {}: {}",
                self.endpoint, e
            );
            return Err(Error::NetworkOperationFailed)
        }

        match reply.unwrap() {
            JsonResult::Response(r) => {
                debug!(target: "rpc::client", "<-- {}", serde_json::to_string(&r)?);

                // Check if the IDs match
                match r.id.as_u64() {
                    Some(id) => {
                        if id != req_id {
                            let e = JsonError::new(ErrorCode::InvalidId, None, r.id);
                            return Err(Error::JsonRpcError(e.error.message.to_string()))
                        }
                    }

                    None => {
                        let e = JsonError::new(ErrorCode::InvalidId, None, r.id);
                        return Err(Error::JsonRpcError(e.error.message.to_string()))
                    }
                }

                Ok(r.result)
            }

            JsonResult::Error(e) => {
                debug!(target: "rpc::client", "<-- {}", serde_json::to_string(&e)?);
                Err(Error::JsonRpcError(e.error.message.to_string()))
            }

            JsonResult::Notification(n) => {
                debug!(target: "rpc::client", "<-- {}", serde_json::to_string(&n)?);
                Err(Error::JsonRpcError("Unexpected reply".to_string()))
            }

            JsonResult::Subscriber(_) => {
                // When?
                Err(Error::JsonRpcError("Unexpected reply".to_string()))
            }
        }
    }

    /// Oneshot send a given JSON-RPC request over the instantiated client
    /// and immediately close the channels upon receiving a reply.
    pub async fn oneshot_request(&self, value: JsonRequest) -> Result<Value> {
        let rep = self.request(value).await?;
        self.stop_signal.send(()).await?;
        Ok(rep)
    }
}
