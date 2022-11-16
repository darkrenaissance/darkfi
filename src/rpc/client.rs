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

//! JSON-RPC client-side implementation.
use std::time::Duration;

use async_std::io::timeout;
use futures::{select, AsyncReadExt, AsyncWriteExt, FutureExt};
use log::{debug, error};
use serde_json::{json, Value};
use url::Url;

use super::jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult};
use crate::{
    net::transport::{
        TcpTransport, TorTransport, Transport, TransportName, TransportStream, UnixTransport,
    },
    Error, Result,
};

/// JSON-RPC client implementation using asynchronous channels.
pub struct RpcClient {
    send: smol::channel::Sender<Value>,
    recv: smol::channel::Receiver<JsonResult>,
    stop_signal: smol::channel::Sender<()>,
    url: Url,
}

impl RpcClient {
    /// Instantiate a new JSON-RPC client that will connect to the given URL.
    pub async fn new(url: Url) -> Result<Self> {
        let (send, recv, stop_signal) = Self::open_channels(&url).await?;
        Ok(Self { send, recv, stop_signal, url })
    }

    /// Close the channels of an instantiated [`RpcClient`].
    pub async fn close(&self) -> Result<()> {
        self.stop_signal.send(()).await?;
        Ok(())
    }

    /// Send a given JSON-RPC request over the instantiated client.
    pub async fn request(&self, value: JsonRequest) -> Result<Value> {
        let req_id = value.id.clone().as_u64().unwrap();

        debug!(target: "jsonrpc-client", "--> {}", serde_json::to_string(&value)?);

        // If the connection is closed, the sender will get an error for
        // sending to a closed channel.
        if let Err(e) = self.send.send(json!(value)).await {
            error!("JSON-RPC client unable to send to {} (channels closed): {}", self.url, e);
            return Err(Error::NetworkOperationFailed)
        }

        // If the connection is closed, the receiver will get an error for
        // waiting on a closed channel.
        let reply = self.recv.recv().await;
        if reply.is_err() {
            error!("JSON-RPC client unable to recv from {} (channels closed)", self.url);
            return Err(Error::NetworkOperationFailed)
        }

        match reply? {
            JsonResult::Response(r) => {
                // Check if the IDs match
                let resp_id = r.id.as_u64();
                if resp_id.is_none() {
                    let e = JsonError::new(ErrorCode::InvalidId, None, r.id);
                    //self.stop_signal.send(()).await?;
                    return Err(Error::JsonRpcError(e.error.message.to_string()))
                }

                if resp_id.unwrap() != req_id {
                    let e = JsonError::new(ErrorCode::InvalidId, None, r.id);
                    //self.stop_signal.send(()).await?;
                    return Err(Error::JsonRpcError(e.error.message.to_string()))
                }

                debug!(target: "jsonrpc-client", "<-- {}", serde_json::to_string(&r)?);
                Ok(r.result)
            }
            JsonResult::Error(e) => {
                debug!(target: "jsonrpc-client", "<-- {}", serde_json::to_string(&e)?);
                // Close the server connection
                //self.stop_signal.send(()).await?;
                Err(Error::JsonRpcError(e.error.message.to_string()))
            }
            JsonResult::Notification(n) => {
                debug!(target: "jsonrpc-client", "<-- {}", serde_json::to_string(&n)?);
                // Close the server connection
                //self.stop_signal.send(()).await?;
                Err(Error::JsonRpcError("Unexpected reply".to_string()))
            }
        }
    }

    /// Oneshot send a given JSON-RPC request over the instantiated client
    /// and close the channels on reply.
    pub async fn oneshot_request(&self, value: JsonRequest) -> Result<Value> {
        let rep = self.request(value).await?;
        self.stop_signal.send(()).await?;
        Ok(rep)
    }

    /// Instantiate channels for a new [`RpcClient`].
    async fn open_channels(
        uri: &Url,
    ) -> Result<(
        smol::channel::Sender<Value>,
        smol::channel::Receiver<JsonResult>,
        smol::channel::Sender<()>,
    )> {
        let (data_send, data_recv) = smol::channel::unbounded();
        let (result_send, result_recv) = smol::channel::unbounded();
        let (stop_send, stop_recv) = smol::channel::unbounded();

        let transport_name = TransportName::try_from(uri.clone())?;

        macro_rules! reqrep {
            ($stream:expr, $transport:expr, $upgrade:expr) => {{
                if let Err(err) = $stream {
                    error!("JSON-RPC client setup for {} failed: {}", uri, err);
                    return Err(Error::ConnectFailed)
                }

                let stream = $stream?.await;
                if let Err(err) = stream {
                    error!("JSON-RPC client connection to {} failed: {}", uri, err);
                    return Err(Error::ConnectFailed)
                }

                let stream = stream?;
                match $upgrade {
                    None => {
                        smol::spawn(Self::reqrep_loop(stream, result_send, data_recv, stop_recv))
                            .detach();
                    }
                    Some(u) if u == "tls" => {
                        let stream = $transport.upgrade_dialer(stream)?.await?;
                        smol::spawn(Self::reqrep_loop(stream, result_send, data_recv, stop_recv))
                            .detach();
                    }
                    Some(u) => return Err(Error::UnsupportedTransportUpgrade(u)),
                }
            }};
        }

        match transport_name {
            TransportName::Tcp(upgrade) => {
                let transport = TcpTransport::new(None, 1024);
                let stream = transport.dial(uri.clone(), None);
                reqrep!(stream, transport, upgrade);
            }
            TransportName::Tor(upgrade) => {
                let socks5_url = TorTransport::get_dialer_env()?;
                let transport = TorTransport::new(socks5_url, None)?;
                let stream = transport.clone().dial(uri.clone(), None);
                reqrep!(stream, transport, upgrade);
            }
            TransportName::Unix => {
                let transport = UnixTransport::new();
                let stream = transport.dial(uri.clone()).await;
                if let Err(err) = stream {
                    error!("JSON-RPC client connection to {} failed: {}", uri, err);
                    return Err(Error::ConnectFailed)
                }

                smol::spawn(Self::reqrep_loop(stream?, result_send, data_recv, stop_recv)).detach();
            }
            _ => unimplemented!(),
        }

        Ok((data_send, result_recv, stop_send))
    }

    /// Internal function that loops on a given stream and multiplexes the data.
    async fn reqrep_loop<T: TransportStream>(
        mut stream: T,
        result_send: smol::channel::Sender<JsonResult>,
        data_recv: smol::channel::Receiver<Value>,
        stop_recv: smol::channel::Receiver<()>,
    ) -> Result<()> {
        // If we don't get a reply within 30 seconds, we'll fail.
        let read_timeout = Duration::from_secs(30);

        loop {
            // Nasty size
            let mut buf = vec![0; 2048 * 10];

            select! {
                data = data_recv.recv().fuse() => {
                    let data_bytes = serde_json::to_vec(&data?)?;
                    stream.write_all(&data_bytes).await?;
                    let n = timeout(read_timeout, async { stream.read(&mut buf[..]).await }).await?;
                    let reply: JsonResult = serde_json::from_slice(&buf[0..n])?;
                    result_send.send(reply).await?;
                }

                _ = stop_recv.recv().fuse() => break
            }
        }

        Ok(())
    }
}
