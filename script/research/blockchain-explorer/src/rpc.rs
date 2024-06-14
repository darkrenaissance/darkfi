/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::{collections::HashSet, sync::Arc, time::Instant};

use async_trait::async_trait;
use log::{debug, error, info, warn};
use smol::lock::MutexGuard;
use tinyjson::JsonValue;
use url::Url;

use darkfi::{
    blockchain::BlockInfo,
    rpc::{
        client::RpcClient,
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::RequestHandler,
    },
    system::{Publisher, StoppableTask, StoppableTaskPtr},
    util::encoding::base64,
    Error, Result,
};
use darkfi_serial::deserialize_async;
use drk::error::{WalletDbError, WalletDbResult};

use crate::{
    blocks::BlockRecord,
    error::{server_error, RpcError},
    BlockchainExplorer,
};

#[async_trait]
impl RequestHandler for BlockchainExplorer {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "blockchain-explorer::rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            // =====================
            // Miscellaneous methods
            // =====================
            "ping" => self.pong(req.id, req.params).await,
            "ping_darkfid" => self.ping_darkfid(req.id, req.params).await,

            // TODO: add statistics retrieval method
            // TODO: add last n blocks retrieval method
            // TODO: add block retrieval method by its header hash
            // TODO: add transactions retrieval method by their block hash
            // TODO: add transaction retrieval method by its hash
            // TODO: add any other usefull method

            // ==============
            // Invalid method
            // ==============
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'_, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

impl BlockchainExplorer {
    // RPCAPI:
    // Pings configured darkfid daemon for liveness.
    // Returns `true` on success.
    //
    // --> {"jsonrpc": "2.0", "method": "ping_darkfid", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "true", "id": 1}
    async fn ping_darkfid(&self, id: u16, _params: JsonValue) -> JsonResult {
        debug!(target: "blockchain-explorer::rpc::ping_darkfid", "Pinging darkfid daemon...");
        if let Err(e) = self.darkfid_daemon_request("ping", &JsonValue::Array(vec![])).await {
            error!(target: "blockchain-explorer::rpc::ping_darkfid", "Failed to ping darkfid daemon: {}", e);
            return server_error(RpcError::PingFailed, id, None)
        }
        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    /// Auxiliary function to execute a request towards the configured darkfid daemon JSON-RPC endpoint.
    pub async fn darkfid_daemon_request(
        &self,
        method: &str,
        params: &JsonValue,
    ) -> Result<JsonValue> {
        debug!(target: "blockchain-explorer::rpc::darkfid_daemon_request", "Executing request {} with params: {:?}", method, params);
        let latency = Instant::now();
        let req = JsonRequest::new(method, params.clone());
        let rep = self.rpc_client.request(req).await?;
        let latency = latency.elapsed();
        debug!(target: "blockchain-explorer::rpc::darkfid_daemon_request", "Got reply: {:?}", rep);
        debug!(target: "blockchain-explorer::rpc::darkfid_daemon_request", "Latency: {:?}", latency);
        Ok(rep)
    }

    // Queries darkfid for a block with given height.
    async fn get_block_by_height(&self, height: u32) -> Result<BlockInfo> {
        let params = self
            .darkfid_daemon_request(
                "blockchain.get_block",
                &JsonValue::Array(vec![JsonValue::String(height.to_string())]),
            )
            .await?;
        let param = params.get::<String>().unwrap();
        let bytes = base64::decode(param).unwrap();
        let block = deserialize_async(&bytes).await?;
        Ok(block)
    }

    /// Syncs the blockchain starting from the last synced block.
    /// If reset flag is provided, all tables are reset, and start scanning from beginning.
    pub async fn sync_blocks(&self, reset: bool) -> WalletDbResult<()> {
        // Grab last scanned block height
        let mut height = self.last_block().await?;
        // If last scanned block is genesis (0) or reset flag
        // has been provided we reset, otherwise continue with
        // the next block height
        if height == 0 || reset {
            self.reset_blocks()?;
            height = 0;
        } else {
            height += 1;
        };

        loop {
            let rep = match self
                .darkfid_daemon_request("blockchain.last_known_block", &JsonValue::Array(vec![]))
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    error!(target: "blockchain-explorer::rpc::sync_blocks", "[sync_blocks] RPC client request failed: {e:?}");
                    return Err(WalletDbError::GenericError)
                }
            };
            let last = *rep.get::<f64>().unwrap() as u32;

            info!(target: "blockchain-explorer::rpc::sync_blocks", "Requested to scan from block number: {height}");
            info!(target: "blockchain-explorer::rpc::sync_blocks", "Last known block number reported by darkfid: {last}");

            // Already scanned last known block
            if height > last {
                return Ok(())
            }

            while height <= last {
                info!(target: "blockchain-explorer::rpc::sync_blocks", "Requesting block {height}... ");

                let block = match self.get_block_by_height(height).await {
                    Ok(r) => r,
                    Err(e) => {
                        error!(target: "blockchain-explorer::rpc::sync_blocks", "[sync_blocks] RPC client request failed: {e:?}");
                        return Err(WalletDbError::GenericError)
                    }
                };

                let block = BlockRecord {
                    header_hash: block.hash().to_string(),
                    version: block.header.version,
                    previous: block.header.previous.to_string(),
                    height: block.header.height,
                    timestamp: block.header.timestamp.inner(),
                    nonce: block.header.nonce,
                    root: block.header.root.to_string(),
                    signature: block.signature,
                };
                if let Err(e) = self.put_block(&block).await {
                    error!(target: "blockchain-explorer::rpc::sync_blocks", "[sync_blocks] Scan block failed: {e:?}");
                    return Err(WalletDbError::GenericError)
                };

                height += 1;
            }
        }
    }
}

/// Subscribes to darkfid's JSON-RPC notification endpoint that serves
/// new finalized blocks. Upon receiving them, store them to the database.
pub async fn subscribe_blocks(
    explorer: Arc<BlockchainExplorer>,
    endpoint: Url,
    ex: Arc<smol::Executor<'static>>,
) -> Result<(StoppableTaskPtr, StoppableTaskPtr)> {
    let rep = explorer
        .darkfid_daemon_request("blockchain.last_known_block", &JsonValue::Array(vec![]))
        .await?;
    let last_known = *rep.get::<f64>().unwrap() as u32;
    let last_scanned = match explorer.last_block().await {
        Ok(l) => l,
        Err(e) => {
            return Err(Error::RusqliteError(format!(
                "[subscribe_blocks] Retrieving last scanned block failed: {e:?}"
            )))
        }
    };

    if last_known != last_scanned {
        warn!(target: "blockchain-explorer::rpc::subscribe_blocks", "Warning: Last scanned block is not the last known block.");
        warn!(target: "blockchain-explorer::rpc::subscribe_blocks", "You should first fully scan the blockchain, and then subscribe");
        return Err(Error::RusqliteError(
            "[subscribe_blocks] Blockchain not fully scanned".to_string(),
        ))
    }

    info!(target: "blockchain-explorer::rpc::subscribe_blocks", "Subscribing to receive notifications of incoming blocks");
    let publisher = Publisher::new();
    let subscription = publisher.clone().subscribe().await;
    let _ex = ex.clone();
    let subscriber_task = StoppableTask::new();
    subscriber_task.clone().start(
        // Weird hack to prevent lifetimes hell
        async move {
            let ex = _ex.clone();
            let rpc_client = RpcClient::new(endpoint, ex).await?;
            let req = JsonRequest::new("blockchain.subscribe_blocks", JsonValue::Array(vec![]));
            rpc_client.subscribe(req, publisher).await
        },
        |res| async move {
            match res {
                Ok(()) => { /* Do nothing */ }
                Err(e) => error!(target: "blockchain-explorer::rpc::subscribe_blocks", "[subscribe_blocks] JSON-RPC server error: {e:?}"),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );
    info!(target: "blockchain-explorer::rpc::subscribe_blocks", "Detached subscription to background");
    info!(target: "blockchain-explorer::rpc::subscribe_blocks", "All is good. Waiting for block notifications...");

    let listener_task = StoppableTask::new();
    listener_task.clone().start(
        // Weird hack to prevent lifetimes hell
        async move {
            loop {
                match subscription.receive().await {
                    JsonResult::Notification(n) => {
                        info!(target: "blockchain-explorer::rpc::subscribe_blocks", "Got Block notification from darkfid subscription");
                        if n.method != "blockchain.subscribe_blocks" {
                            return Err(Error::UnexpectedJsonRpc(format!(
                                "Got foreign notification from darkfid: {}",
                                n.method
                            )))
                        }

                        // Verify parameters
                        if !n.params.is_array() {
                            return Err(Error::UnexpectedJsonRpc(
                                "Received notification params are not an array".to_string(),
                            ))
                        }
                        let params = n.params.get::<Vec<JsonValue>>().unwrap();
                        if params.is_empty() {
                            return Err(Error::UnexpectedJsonRpc(
                                "Notification parameters are empty".to_string(),
                            ))
                        }

                        for param in params {
                            let param = param.get::<String>().unwrap();
                            let bytes = base64::decode(param).unwrap();

                            let block_data: BlockInfo = match deserialize_async(&bytes).await {
                                Ok(b) => b,
                                Err(e) => {
                                    return Err(Error::UnexpectedJsonRpc(format!(
                                        "[subscribe_blocks] Deserializing block failed: {e:?}"
                                    )))
                                },
                            };
                            let header_hash = block_data.hash().to_string();
                            info!(target: "blockchain-explorer::rpc::subscribe_blocks", "=======================================");
                            info!(target: "blockchain-explorer::rpc::subscribe_blocks", "Block header: {header_hash}");
                            info!(target: "blockchain-explorer::rpc::subscribe_blocks", "=======================================");

                            info!(target: "blockchain-explorer::rpc::subscribe_blocks", "Deserialized successfully. Storring block...");
                            let block = BlockRecord {
                                header_hash,
                                version: block_data.header.version,
                                previous: block_data.header.previous.to_string(),
                                height: block_data.header.height,
                                timestamp: block_data.header.timestamp.inner(),
                                nonce: block_data.header.nonce,
                                root: block_data.header.root.to_string(),
                                signature: block_data.signature,
                            };
                            if let Err(e) = explorer.put_block(&block).await {
                                return Err(Error::RusqliteError(format!(
                                    "[subscribe_blocks] Scanning block failed: {e:?}"
                                )))
                            }
                        }
                    }

                    JsonResult::Error(e) => {
                        // Some error happened in the transmission
                        return Err(Error::UnexpectedJsonRpc(format!("Got error from JSON-RPC: {e:?}")))
                    }

                    x => {
                        // And this is weird
                        return Err(Error::UnexpectedJsonRpc(format!(
                            "Got unexpected data from JSON-RPC: {x:?}"
                        )))
                    }
                }
            };
        },
        |res| async move {
            match res {
                Ok(()) => { /* Do nothing */ }
                Err(e) => error!(target: "blockchain-explorer::rpc::subscribe_blocks", "[subscribe_blocks] JSON-RPC server error: {e:?}"),
            }
        },
        Error::RpcServerStopped,
        ex,
    );

    Ok((subscriber_task, listener_task))
}
