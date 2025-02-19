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

use std::sync::Arc;

use log::{debug, error, info, warn};
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

use crate::{error::handle_database_error, Explorerd};

impl Explorerd {
    // Queries darkfid for a block with given height.
    async fn get_darkfid_block_by_height(&self, height: u32) -> Result<BlockInfo> {
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

    /// Synchronizes blocks between the explorer and a Darkfi blockchain node, ensuring
    /// the database remains consistent by syncing any missing or outdated blocks.
    ///
    /// If provided `reset` is true, the explorer's blockchain-related and metric sled trees are purged
    /// and syncing starts from the genesis block. The function also handles reorgs by re-aligning the
    /// explorer state to the correct height when blocks are outdated. Returns a result indicating
    /// success or failure.
    ///
    /// Reorg handling is delegated to the [`Self::process_sync_blocks_reorg`] function, whose
    /// documentation provides more details on the reorg process during block syncing.
    pub async fn sync_blocks(&self, reset: bool) -> Result<()> {
        // Grab last synced block height from the explorer's database.
        let last_synced_block = self.service.last_block().map_err(|e| {
            handle_database_error(
                "rpc_blocks::sync_blocks",
                "[sync_blocks] Retrieving last synced block failed",
                e,
            )
        })?;

        // Grab the last confirmed block height and hash from the darkfi node
        let (last_darkfid_height, last_darkfid_hash) = self.get_last_confirmed_block().await?;

        // Initialize the current height to sync from, starting from genesis block if last sync block does not exist
        let (last_synced_height, last_synced_hash) = last_synced_block
            .map_or((0, "".to_string()), |(height, header_hash)| (height, header_hash));

        // Declare a mutable variable to track the current sync height while processing blocks
        let mut current_height = last_synced_height;

        info!(target: "explorerd::rpc_blocks::sync_blocks", "Requested to sync from block number: {current_height}");
        info!(target: "explorerd::rpc_blocks::sync_blocks", "Last confirmed block number reported by darkfid: {last_darkfid_height} - {last_darkfid_hash}");

        // A reorg is detected if the hash of the last synced block differs from the hash of the last confirmed block,
        // unless the reset flag is set or the current height is 0
        let reorg_detected = last_synced_hash != last_darkfid_hash && !reset && current_height != 0;

        // If the reset flag is set, reset the explorer state and start syncing from the genesis block height.
        // Otherwise, handle reorgs if detected, or proceed to the next block if not at the genesis height.
        if reset {
            self.service.reset_explorer_state(0)?;
            current_height = 0;
            info!(target: "explorerd::rpc_blocks::sync_blocks", "Successfully reset explorer database based on set reset parameter");
        } else if reorg_detected {
            current_height =
                self.process_sync_blocks_reorg(last_synced_height, last_darkfid_height).await?;
            // Log only if a reorg occurred
            if current_height != last_synced_height {
                info!(target: "explorerd::rpc_blocks::sync_blocks", "Successfully completed reorg to height: {current_height}");
            }
            // Prepare to sync the next block after reorg if not from genesis height
            if current_height != 0 {
                current_height += 1;
            }
        } else if current_height != 0 {
            // Resume syncing from the block after the last synced height
            current_height += 1;
        }

        // Sync blocks until the explorer is up to date with the last confirmed block
        while current_height <= last_darkfid_height {
            // Retrieve the block from darkfi node by height
            let block = match self.get_darkfid_block_by_height(current_height).await {
                Ok(r) => r,
                Err(e) => {
                    return Err(handle_database_error(
                        "rpc_blocks::sync_blocks",
                        "[sync_blocks] RPC client request failed",
                        e,
                    ))
                }
            };

            // Store the retrieved block in the explorer's database
            if let Err(e) = self.service.put_block(&block).await {
                return Err(handle_database_error(
                    "rpc_blocks::sync_blocks",
                    "[sync_blocks] Put block failed",
                    e,
                ))
            };

            info!(target: "explorerd::rpc_blocks::sync_blocks", "Synced block {current_height}");

            // Increment the current height to sync the next block
            current_height += 1;
        }

        info!(target: "explorerd::rpc_blocks::sync_blocks", "Completed sync, total number of explorer blocks: {}", self.service.db.blockchain.blocks.len());

        Ok(())
    }

    /// Handles blockchain reorganizations (reorgs) during the explorer node's startup synchronization
    /// with Darkfi nodes, ensuring the explorer provides a consistent and accurate view of the blockchain.
    ///
    /// A reorg occurs when the blocks stored by the blockchain nodes diverge from those stored by the explorer.
    /// This function resolves inconsistencies by identifying the point of divergence, searching backward through
    /// block heights, and comparing block hashes between the explorer database and the blockchain node. Once a
    /// common block height is found, the explorer is re-aligned to that height.
    ///
    /// If no common block can be found, the explorer resets to the "genesis height," removing all blocks,
    /// transactions, and metrics from its database to resynchronize with the canonical chain from the nodes.
    ///
    /// Returns the last height at which the explorer's state was successfully re-aligned with the blockchain.
    async fn process_sync_blocks_reorg(
        &self,
        last_synced_height: u32,
        last_darkfid_height: u32,
    ) -> Result<u32> {
        // Log reorg detection in the case that explorer height is greater or equal to height of darkfi node
        if last_synced_height >= last_darkfid_height {
            info!(target: "explorerd::rpc_blocks::process_sync_blocks_reorg",
                "Reorg detected with heights: explorer.{last_synced_height} >= darkfid.{last_darkfid_height}");
        }

        // Declare a mutable variable to track the current height while searching for a common block
        let mut cur_height = last_synced_height;
        // Search for an explorer block that matches a darkfi node block
        while cur_height > 0 {
            let synced_block = self.service.get_block_by_height(cur_height)?;
            debug!(target: "explorerd::rpc_blocks::process_sync_blocks_reorg", "Searching for common block: {}", cur_height);

            // Check if we found a synced block for current height being searched
            if let Some(synced_block) = synced_block {
                // Fetch the block from darkfi node to check for a match
                match self.get_darkfid_block_by_height(cur_height).await {
                    Ok(darkfid_block) => {
                        // If hashes match, we've found the point of divergence
                        if synced_block.header_hash == darkfid_block.hash().to_string() {
                            // If hashes match but the cur_height differs from the last synced height, reset the explorer state
                            if cur_height != last_synced_height {
                                self.service.reset_explorer_state(cur_height)?;
                                debug!(target: "explorerd::rpc_blocks::process_sync_blocks_reorg", "Successfully completed reorg to height: {cur_height}");
                            }
                            break;
                        } else {
                            // Log reorg detection with height and header hash mismatch details
                            if cur_height == last_synced_height {
                                info!(
                                    target: "explorerd::rpc_blocks::process_sync_blocks_reorg",
                                    "Reorg detected at height {}: explorer.{} != darkfid.{}",
                                    cur_height,
                                    synced_block.header_hash,
                                    darkfid_block.hash().to_string()
                                );
                            }
                        }
                    }
                    // Continue searching for blocks that do not exist on darkfi nodes
                    Err(Error::JsonRpcError((-32121, _))) => (),
                    Err(e) => {
                        return Err(handle_database_error(
                            "rpc_blocks::process_sync_blocks_reorg",
                            "[process_sync_blocks_reorg] RPC client request failed",
                            e,
                        ))
                    }
                }
            }

            // Move to previous block to search for a match
            cur_height = cur_height.saturating_sub(1);
        }

        // Check if genesis block reorg is needed
        if cur_height == 0 {
            self.service.reset_explorer_state(0)?;
        }

        // Return the last height we reorged to
        Ok(cur_height)
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

        // Extract the number of last blocks to retrieve from parameters
        let n = match params[0].get::<String>().unwrap().parse::<usize>() {
            Ok(v) => v,
            Err(_) => return JsonError::new(ParseError, None, id).into(),
        };

        // Fetch the blocks and handle potential errors
        let blocks_result = match self.service.get_last_n(n) {
            Ok(blocks) => blocks,
            Err(e) => {
                error!(target: "explorerd::rpc_blocks::blocks_get_last_n_blocks", "Failed fetching blocks: {}", e);
                return JsonError::new(InternalError, None, id).into();
            }
        };

        // Transform blocks to json and return result
        if blocks_result.is_empty() {
            JsonResponse::new(JsonValue::Array(vec![]), id).into()
        } else {
            let json_blocks: Vec<JsonValue> =
                blocks_result.into_iter().map(|block| block.to_json_array()).collect();
            JsonResponse::new(JsonValue::Array(json_blocks), id).into()
        }
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

        // Fetch the blocks and handle potential errors
        let blocks_result = match self.service.get_by_range(start, end) {
            Ok(blocks) => blocks,
            Err(e) => {
                error!(target: "explorerd::rpc_blocks::blocks_get_blocks_in_height_range", "Failed fetching blocks: {}", e);
                return JsonError::new(InternalError, None, id).into();
            }
        };

        // Transform blocks to json and return result
        if blocks_result.is_empty() {
            JsonResponse::new(JsonValue::Array(vec![]), id).into()
        } else {
            let json_blocks: Vec<JsonValue> =
                blocks_result.into_iter().map(|block| block.to_json_array()).collect();
            JsonResponse::new(JsonValue::Array(json_blocks), id).into()
        }
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

        // Extract header hash from params, returning error if not provided
        let header_hash = match params[0].get::<String>() {
            Some(hash) => hash,
            None => return JsonError::new(InvalidParams, None, id).into(),
        };

        // Fetch and transform block to json, handling any errors and returning the result
        match self.service.get_block_by_hash(header_hash) {
            Ok(Some(block)) => JsonResponse::new(block.to_json_array(), id).into(),
            Ok(None) => JsonResponse::new(JsonValue::Array(vec![]), id).into(),
            Err(e) => {
                error!(target: "explorerd::rpc_blocks", "Failed fetching block: {:?}", e);
                JsonError::new(InternalError, None, id).into()
            }
        }
    }

    // Queries darkfid for last confirmed block.
    async fn get_last_confirmed_block(&self) -> Result<(u32, String)> {
        let rep = self
            .darkfid_daemon_request("blockchain.last_confirmed_block", &JsonValue::Array(vec![]))
            .await?;
        let params = rep.get::<Vec<JsonValue>>().unwrap();
        let height = *params[0].get::<f64>().unwrap() as u32;
        let hash = params[1].get::<String>().unwrap().clone();

        Ok((height, hash))
    }
}

/// Subscribes to darkfid's JSON-RPC notification endpoint that serves
/// new confirmed blocks. Upon receiving them, store them to the database.
pub async fn subscribe_blocks(
    explorer: Arc<Explorerd>,
    endpoint: Url,
    ex: Arc<smol::Executor<'static>>,
) -> Result<(StoppableTaskPtr, StoppableTaskPtr)> {
    // Grab last confirmed block
    let (last_darkfid_height, last_darkfid_hash) = explorer.get_last_confirmed_block().await?;

    // Grab last synced block
    let (mut height, hash) = match explorer.service.last_block() {
        Ok(Some((height, hash))) => (height, hash),
        Ok(None) => (0, "".to_string()),
        Err(e) => {
            return Err(Error::DatabaseError(format!(
                "[subscribe_blocks] Retrieving last synced block failed: {e:?}"
            )))
        }
    };

    // Evaluates whether there is a mismatch between the last confirmed block and the last synced block
    let blocks_mismatch = (last_darkfid_height != height || last_darkfid_hash != hash) &&
        last_darkfid_height != 0 &&
        height != 0;

    // Check if there is a mismatch, throwing an error to prevent operating in a potentially inconsistent state
    if blocks_mismatch {
        warn!(target: "explorerd::rpc_blocks::subscribe_blocks",
        "Warning: Last synced block is not the last confirmed block: \
        last_darkfid_height={last_darkfid_height}, last_synced_height={height}, last_darkfid_hash={last_darkfid_hash}, last_synced_hash={hash}");
        warn!(target: "explorerd::rpc_blocks::subscribe_blocks", "You should first fully sync the blockchain, and then subscribe");
        return Err(Error::DatabaseError(
            "[subscribe_blocks] Blockchain not fully synced".to_string(),
        ));
    }

    info!(target: "explorerd::rpc_blocks::subscribe_blocks", "Subscribing to receive notifications of incoming blocks");
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
                Err(e) => error!(target: "explorerd::rpc_blocks::subscribe_blocks", "[subscribe_blocks] JSON-RPC server error: {e:?}"),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );
    info!(target: "explorerd::rpc_blocks::subscribe_blocks", "Detached subscription to background");
    info!(target: "explorerd::rpc_blocks::subscribe_blocks", "All is good. Waiting for block notifications...");

    let listener_task = StoppableTask::new();
    listener_task.clone().start(
        // Weird hack to prevent lifetimes hell
        async move {
            loop {
                match subscription.receive().await {
                    JsonResult::Notification(n) => {
                        debug!(target: "explorerd::rpc_blocks::subscribe_blocks", "Got Block notification from darkfid subscription");
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

                            let darkfid_block: BlockInfo = match deserialize_async(&bytes).await {
                                Ok(b) => b,
                                Err(e) => {
                                    return Err(Error::UnexpectedJsonRpc(format!(
                                        "[subscribe_blocks] Deserializing block failed: {e:?}"
                                    )))
                                },
                            };
                            info!(target: "explorerd::rpc_blocks::subscribe_blocks", "=======================================");
                            info!(target: "explorerd::rpc_blocks::subscribe_blocks", "Block Notification: {}", darkfid_block.hash().to_string());
                            info!(target: "explorerd::rpc_blocks::subscribe_blocks", "=======================================");

                            // Store darkfi node block height for later use
                            let darkfid_block_height = darkfid_block.header.height;

                            // Check if we need to perform a reorg due to mismatch in block heights
                            if darkfid_block_height <= height {
                                info!(target: "explorerd::rpc_blocks::subscribe_blocks",
                                    "Reorg detected with heights: darkfid.{darkfid_block_height} <= explorer.{height}");

                                // Calculate the reset height
                                let reset_height = darkfid_block_height.saturating_sub(1);

                                // Execute the reorg by resetting the explorer state to reset height
                                explorer.service.reset_explorer_state(reset_height)?;
                                info!(target: "explorerd::rpc_blocks::subscribe_blocks", "Successfully completed reorg to height: {reset_height}");
                            }

                            if let Err(e) = explorer.service.put_block(&darkfid_block).await {
                                return Err(Error::DatabaseError(format!(
                                    "[subscribe_blocks] Put block failed: {e:?}"
                                )))
                            }

                            info!(target: "explorerd::rpc_blocks::subscribe_blocks", "Successfully stored new block at height: {}", darkfid_block.header.height );

                            // Process the next block
                            height = darkfid_block.header.height;
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
                Err(e) => error!(target: "explorerd::rpc_blocks::subscribe_blocks", "[subscribe_blocks] JSON-RPC server error: {e:?}"),
            }
        },
        Error::RpcServerStopped,
        ex,
    );

    Ok((subscriber_task, listener_task))
}
