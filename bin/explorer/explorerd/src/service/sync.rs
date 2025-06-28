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

//! # Sync Module
//!
//! The `sync` module is responsible for synchronizing the explorer's database with the Darkfi
//! blockchain network. It ensures consistency between the explorer and the blockchain by
//! fetching missing blocks, handling reorganizations (reorgs), and subscribing to live updates
//! through Darkfi's JSON-RPC service.
//!
//! ## Responsibilities
//!
//! - **Block Synchronization**: Handles fetching and storing blocks from a Darkfi
//!   blockchain node during startup or when syncing, ensuring the explorer stays synchronized
//!   with the latest confirmed blocks.
//! - **Real-Time Updates**: Subscribes to Darkfi's JSON-RPC notification service,
//!   allowing the explorer to process and sync new blocks as they are confirmed.
//! - **Reorg Handling**: Detects and resolves blockchain reorganizations by identifying
//!   the last common block (in case of divergence) and re-aligning the explorer's state with the
//!   latest blockchain state. Reorgs are an importnt part of synchronization because they prevent
//!   syncing invalid or outdated states, ensuring the explorer maintains an accurate view of a
//!   Darkfi blockchain network.

use std::{sync::Arc, time::Instant};

use log::{debug, error, info, warn};
use tinyjson::JsonValue;
use url::Url;

use darkfi::{
    blockchain::BlockInfo,
    rpc::{
        client::RpcClient,
        jsonrpc::{JsonRequest, JsonResult},
    },
    system::{Publisher, StoppableTask, StoppableTaskPtr},
    util::{encoding::base64, time::fmt_duration},
    Error,
};
use darkfi_serial::deserialize_async;

use crate::{error::handle_database_error, service::ExplorerService, Explorerd};

impl ExplorerService {
    /// Synchronizes blocks between the explorer and a Darkfi blockchain node, ensuring
    /// the database remains consistent by syncing any missing or outdated blocks.
    ///
    /// If provided `reset` is true, the explorer's blockchain-related and metric sled trees are purged
    /// and syncing starts from the genesis block. The function also handles reorgs by re-aligning the
    /// explorer state to the correct height when blocks are outdated. Returns a result indicating
    /// success or failure.
    ///
    /// Reorg handling is delegated to the [`Self::reorg_blocks`] function, whose
    /// documentation provides more details on the reorg process during block syncing.
    pub async fn sync_blocks(&self, reset: bool) -> darkfi::Result<()> {
        // Grab last synced block height from the explorer's database.
        let last_synced_block = self.last_block().map_err(|e| {
            handle_database_error(
                "rpc_blocks::sync_blocks",
                "[sync_blocks] Retrieving last synced block failed",
                e,
            )
        })?;

        // Grab the last confirmed block height and hash from the darkfi node
        let (last_darkfid_height, last_darkfid_hash) =
            self.darkfid_client.get_last_confirmed_block().await?;

        // Initialize the current height to sync from, starting from genesis block if last sync block does not exist
        let (last_synced_height, last_synced_hash) = last_synced_block
            .map_or((0, "".to_string()), |(height, header_hash)| (height, header_hash));

        // Declare a mutable variable to track the current sync height while processing blocks
        let mut current_height = last_synced_height;

        info!(target: "explorerd::rpc_blocks::sync_blocks", "Syncing from block number: {current_height}");
        info!(target: "explorerd::rpc_blocks::sync_blocks", "Last confirmed darkfid block: {last_darkfid_height} - {last_darkfid_hash}");

        // A reorg is detected if the hash of the last synced block differs from the hash of the last confirmed block,
        // unless the reset flag is set or the current height is 0
        let reorg_detected = last_synced_hash != last_darkfid_hash && !reset && current_height != 0;

        // If the reset flag is set, reset the explorer state and start syncing from the genesis block height.
        // Otherwise, handle reorgs if detected, or proceed to the next block if not at the genesis height.
        if reset {
            self.reset_explorer_state(0)?;
            current_height = 0;
            info!(target: "explorerd::rpc_blocks::sync_blocks", "Reset explorer database based on set reset parameter");
        } else if reorg_detected {
            // Record the start time to measure the duration of potential reorg
            let start_reorg_time = Instant::now();

            // Process reorg
            current_height = self.reorg_blocks(last_synced_height, last_darkfid_height).await?;

            // Log only if a reorg occurred (i.e., the explorer wasn't merely catching up to Darkfi node blocks)
            if current_height != last_synced_height {
                info!(target: "explorerd::rpc_blocks::sync_blocks", "Completed reorg to height: {current_height} [{}]", fmt_duration(start_reorg_time.elapsed()));
            }

            // Prepare to sync the next block after reorg if not from genesis height
            if current_height != 0 {
                current_height += 1;
            }
        } else if current_height != 0 {
            // Resume syncing from the block after the last synced height
            current_height += 1;
        }

        // Record the sync start time to measure the total block sync duration
        let sync_start_time = Instant::now();
        // Track the number of blocks synced for reporting
        let mut blocks_synced = 0;

        // Sync blocks until the explorer is up to date with the last confirmed block
        while current_height <= last_darkfid_height {
            // Record the start time to measure the duration it took to sync the block
            let block_sync_start = Instant::now();

            // Retrieve the block from darkfi node by height
            let block = match self.darkfid_client.get_block_by_height(current_height).await {
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
            if let Err(e) = self.put_block(&block).await {
                return Err(handle_database_error(
                    "rpc_blocks::sync_blocks",
                    "[sync_blocks] Put block failed",
                    e,
                ));
            };

            debug!(
                target: "explorerd::rpc_blocks::sync_blocks",
                "Synced block {current_height} [{}]",
                fmt_duration(block_sync_start.elapsed())
            );

            // Increment the current height to sync the next block
            current_height += 1;
            // Increment the count of successfully synced blocks
            blocks_synced += 1;
        }

        info!(
            target: "explorerd::rpc_blocks::sync_blocks",
            "Synced {blocks_synced} blocks: explorer blocks total {} [{}]",
            self.db.blockchain.blocks.len(),
            fmt_duration(sync_start_time.elapsed()),
        );

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
    async fn reorg_blocks(
        &self,
        last_synced_height: u32,
        last_darkfid_height: u32,
    ) -> darkfi::Result<u32> {
        // Log reorg detection in the case that explorer height is greater or equal to height of darkfi node
        if last_synced_height >= last_darkfid_height {
            info!(target: "explorerd::rpc_blocks::process_sync_blocks_reorg",
                "Reorg detected with heights: explorer.{last_synced_height} >= darkfid.{last_darkfid_height}");
        }

        // Declare a mutable variable to track the current height while searching for a common block
        let mut cur_height = last_synced_height;
        // Search for an explorer block that matches a darkfi node block
        while cur_height > 0 {
            let synced_block = self.get_block_by_height(cur_height)?;
            debug!(target: "explorerd::rpc_blocks::process_sync_blocks_reorg", "Searching for common block: {cur_height}");

            // Check if we found a synced block for current height being searched
            if let Some(synced_block) = synced_block {
                // Fetch the block from darkfi node to check for a match
                match self.darkfid_client.get_block_by_height(cur_height).await {
                    Ok(darkfid_block) => {
                        // If hashes match, we've found the point of divergence
                        if synced_block.header_hash == darkfid_block.hash().to_string() {
                            // If hashes match but the cur_height differs from the last synced height, reset the explorer state
                            if cur_height != last_synced_height {
                                self.reset_explorer_state(cur_height)?;
                                debug!(target: "explorerd::rpc_blocks::process_sync_blocks_reorg", "Completed reorg to height: {cur_height}");
                            }
                            break;
                        } else {
                            // Log reorg detection with height and header hash mismatch details
                            if cur_height == last_synced_height {
                                info!(
                                    target: "explorerd::rpc_blocks::process_sync_blocks_reorg",
                                    "Reorg detected at height {cur_height}: explorer.{} != darkfid.{}",
                                    synced_block.header_hash,
                                    darkfid_block.hash()
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
            self.reset_explorer_state(0)?;
        }

        // Return the last height we reorged to
        Ok(cur_height)
    }
}
/// Subscribes to darkfid's JSON-RPC notification endpoint that serves
/// new confirmed blocks. Upon receiving them, store them to the database.
pub async fn subscribe_sync_blocks(
    explorer: Arc<Explorerd>,
    endpoint: Url,
    ex: Arc<smol::Executor<'static>>,
) -> darkfi::Result<(StoppableTaskPtr, StoppableTaskPtr)> {
    // Grab last confirmed block
    let (last_darkfid_height, last_darkfid_hash) =
        explorer.darkfid_client.get_last_confirmed_block().await?;

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
                            info!(target: "explorerd::rpc_blocks::subscribe_blocks", "========================================================================================");
                            info!(target: "explorerd::rpc_blocks::subscribe_blocks", "| Block Notification: {} |", darkfid_block.hash());
                            info!(target: "explorerd::rpc_blocks::subscribe_blocks", "========================================================================================");

                            // Store darkfi node block height for later use
                            let darkfid_block_height = darkfid_block.header.height;

                            // Check if we need to perform a reorg due to mismatch in block heights
                            if darkfid_block_height <= height {
                                info!(target: "explorerd::rpc_blocks::subscribe_blocks",
                                    "Reorg detected with heights: darkfid.{darkfid_block_height} <= explorer.{height}");

                                // Calculate the reset height
                                let reset_height = darkfid_block_height.saturating_sub(1);

                                // Record the start time to measure the duration of the reorg
                                let start_reorg_time = Instant::now();

                                // Execute the reorg by resetting the explorer state to reset height
                                explorer.service.reset_explorer_state(reset_height)?;
                                info!(target: "explorerd::rpc_blocks::subscribe_blocks", "Completed reorg to height: {reset_height} [{}]", fmt_duration(start_reorg_time.elapsed()));
                            }


                            // Record the start time to measure the duration to store the block
                            let start_reorg_time = Instant::now();

                            if let Err(e) = explorer.service.put_block(&darkfid_block).await {
                                return Err(Error::DatabaseError(format!(
                                    "[subscribe_blocks] Put block failed: {e:?}"
                                )))
                            }

                            info!(target: "explorerd::rpc_blocks::subscribe_blocks", "Stored new block at height: {} [{}]", darkfid_block.header.height, fmt_duration(start_reorg_time.elapsed()));

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
