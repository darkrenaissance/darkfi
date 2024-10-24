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

use std::sync::Arc;

use log::{error, info, warn};
use tinyjson::JsonValue;
use url::Url;

use darkfi::{
    blockchain::BlockInfo,
    rpc::{
        client::RpcClient,
        jsonrpc::{
            ErrorCode::{InternalError, InvalidParams, ParseError},
            JsonError, JsonRequest, JsonResponse, JsonResult,
        },
    },
    system::{Publisher, StoppableTask, StoppableTaskPtr},
    util::encoding::base64,
    Error, Result,
};
use darkfi_serial::deserialize_async;
use drk::error::{WalletDbError, WalletDbResult};

use crate::BlockchainExplorer;

impl BlockchainExplorer {
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
    /// If reset flag is provided, all tables are reset, and start syncing from beginning.
    pub async fn sync_blocks(&self, reset: bool) -> WalletDbResult<()> {
        // Grab last synced block height
        let (mut height, _) = self.last_block().await?;
        // If last synced block is genesis (0) or reset flag
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
                    error!(target: "blockchain-explorer::rpc_blocks::sync_blocks", "[sync_blocks] RPC client request failed: {e:?}");
                    return Err(WalletDbError::GenericError)
                }
            };
            let last = *rep.get::<f64>().unwrap() as u32;

            info!(target: "blockchain-explorer::rpc_blocks::sync_blocks", "Requested to sync from block number: {height}");
            info!(target: "blockchain-explorer::rpc_blocks::sync_blocks", "Last known block number reported by darkfid: {last}");

            // Already synced last known block
            if height > last {
                return Ok(())
            }

            while height <= last {
                info!(target: "blockchain-explorer::rpc_blocks::sync_blocks", "Requesting block {height}... ");

                let block = match self.get_block_by_height(height).await {
                    Ok(r) => r,
                    Err(e) => {
                        error!(target: "blockchain-explorer::rpc_blocks::sync_blocks", "[sync_blocks] RPC client request failed: {e:?}");
                        return Err(WalletDbError::GenericError)
                    }
                };

                if let Err(e) = self.put_block(&(&block).into()).await {
                    error!(target: "blockchain-explorer::rpc_blocks::sync_blocks", "[sync_blocks] Insert block failed: {e:?}");
                    return Err(WalletDbError::GenericError)
                };

                let block_hash = block.hash().to_string();
                for transaction in block.txs {
                    if let Err(e) = self.put_transaction(&(&block_hash, &transaction).into()).await
                    {
                        error!(target: "blockchain-explorer::rpc_blocks::sync_blocks", "[sync_blocks] Insert block transaction failed: {e:?}");
                        return Err(WalletDbError::GenericError)
                    };
                }

                height += 1;
            }
        }
    }

    // RPCAPI:
    // Queries the database to retrieve last N blocks.
    // Returns an array of readable blocks upon success.
    //
    // **Params:**
    // * `array[0]`: `u16` Number of blocks to retrieve (as string)
    //
    // **Returns:**
    // * Array of `BlockRecord` encoded into a JSON.
    //
    // --> {"jsonrpc": "2.0", "method": "blocks.get_last_n_blocks", "params": ["10"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn blocks_get_last_n_blocks(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let n = match params[0].get::<String>().unwrap().parse::<u16>() {
            Ok(v) => v,
            Err(_) => return JsonError::new(ParseError, None, id).into(),
        };

        let blocks = match self.get_last_n_blocks(n) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "blockchain-explorer::rpc_blocks::blocks_get_last_n_blocks", "Failed fetching blocks: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        let mut ret = vec![];
        for block in blocks {
            ret.push(block.to_json_array());
        }
        JsonResponse::new(JsonValue::Array(ret), id).into()
    }

    // RPCAPI:
    // Queries the database to retrieve blocks in provided heights range.
    // Returns an array of readable blocks upon success.
    //
    // **Params:**
    // * `array[0]`: `u32` Starting height (as string)
    // * `array[1]`: `u32` Ending height range (as string)
    //
    // **Returns:**
    // * Array of `BlockRecord` encoded into a JSON.
    //
    // --> {"jsonrpc": "2.0", "method": "blocks.get_blocks_in_heights_range", "params": ["10", "15"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn blocks_get_blocks_in_heights_range(
        &self,
        id: u16,
        params: JsonValue,
    ) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 2 || !params[0].is_string() || !params[1].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let start = match params[0].get::<String>().unwrap().parse::<u32>() {
            Ok(v) => v,
            Err(_) => return JsonError::new(ParseError, None, id).into(),
        };

        let end = match params[1].get::<String>().unwrap().parse::<u32>() {
            Ok(v) => v,
            Err(_) => return JsonError::new(ParseError, None, id).into(),
        };

        if start > end {
            return JsonError::new(ParseError, None, id).into()
        }

        let blocks = match self.get_blocks_in_heights_range(start, end) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "blockchain-explorer::rpc_blocks::blocks_get_blocks_in_height_range", "Failed fetching blocks: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        let mut ret = vec![];
        for block in blocks {
            ret.push(block.to_json_array());
        }
        JsonResponse::new(JsonValue::Array(ret), id).into()
    }

    // RPCAPI:
    // Queries the database to retrieve the block corresponding to the provided hash.
    // Returns the readable block upon success.
    //
    // **Params:**
    // * `array[0]`: `String` Block header hash
    //
    // **Returns:**
    // * `BlockRecord` encoded into a JSON.
    //
    // --> {"jsonrpc": "2.0", "method": "blocks.get_block_by_hash", "params": ["5cc...2f9"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn blocks_get_block_by_hash(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let header_hash = params[0].get::<String>().unwrap();
        let block = match self.get_block_by_hash(header_hash) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "blockchain-explorer::rpc_blocks::blocks_get_get_block_by_hash", "Failed fetching block: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        JsonResponse::new(block.to_json_array(), id).into()
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
    let (last_synced, _) = match explorer.last_block().await {
        Ok(l) => l,
        Err(e) => {
            return Err(Error::DatabaseError(format!(
                "[subscribe_blocks] Retrieving last synced block failed: {e:?}"
            )))
        }
    };

    if last_known != last_synced {
        warn!(target: "blockchain-explorer::rpc_blocks::subscribe_blocks", "Warning: Last synced block is not the last known block.");
        warn!(target: "blockchain-explorer::rpc_blocks::subscribe_blocks", "You should first fully sync the blockchain, and then subscribe");
        return Err(Error::DatabaseError(
            "[subscribe_blocks] Blockchain not fully synced".to_string(),
        ))
    }

    info!(target: "blockchain-explorer::rpc_blocks::subscribe_blocks", "Subscribing to receive notifications of incoming blocks");
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
                Err(e) => error!(target: "blockchain-explorer::rpc_blocks::subscribe_blocks", "[subscribe_blocks] JSON-RPC server error: {e:?}"),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );
    info!(target: "blockchain-explorer::rpc_blocks::subscribe_blocks", "Detached subscription to background");
    info!(target: "blockchain-explorer::rpc_blocks::subscribe_blocks", "All is good. Waiting for block notifications...");

    let listener_task = StoppableTask::new();
    listener_task.clone().start(
        // Weird hack to prevent lifetimes hell
        async move {
            loop {
                match subscription.receive().await {
                    JsonResult::Notification(n) => {
                        info!(target: "blockchain-explorer::rpc_blocks::subscribe_blocks", "Got Block notification from darkfid subscription");
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
                            info!(target: "blockchain-explorer::rpc_blocks::subscribe_blocks", "=======================================");
                            info!(target: "blockchain-explorer::rpc_blocks::subscribe_blocks", "Block header: {header_hash}");
                            info!(target: "blockchain-explorer::rpc_blocks::subscribe_blocks", "=======================================");

                            info!(target: "blockchain-explorer::rpc_blocks::subscribe_blocks", "Deserialized successfully. Storring block...");
                            if let Err(e) = explorer.put_block(&(&block_data).into()).await {
                                return Err(Error::DatabaseError(format!(
                                    "[subscribe_blocks] Insert block failed: {e:?}"
                                )))
                            }

                            let block_hash = block_data.hash().to_string();
                            for transaction in block_data.txs {
                                if let Err(e) = explorer.put_transaction(&(&block_hash, &transaction).into()).await {
                                    return Err(Error::DatabaseError(format!(
                                        "[subscribe_blocks] Insert block transaction failed: {e:?}"
                                    )))
                                };
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
                Err(e) => error!(target: "blockchain-explorer::rpc_blocks::subscribe_blocks", "[subscribe_blocks] JSON-RPC server error: {e:?}"),
            }
        },
        Error::RpcServerStopped,
        ex,
    );

    Ok((subscriber_task, listener_task))
}
