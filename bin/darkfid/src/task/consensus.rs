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

use std::str::FromStr;

use darkfi::{
    blockchain::HeaderHash,
    rpc::{jsonrpc::JsonNotification, util::JsonValue},
    system::{sleep, ExecutorPtr, StoppableTask, Subscription},
    util::{encoding::base64, time::Timestamp},
    Error, Result,
};
use darkfi_serial::serialize_async;
use tracing::{error, info};

use crate::{
    task::{garbage_collect_task, sync_task},
    DarkfiNodePtr,
};

/// Auxiliary structure representing node consensus init task configuration.
#[derive(Clone)]
pub struct ConsensusInitTaskConfig {
    /// Skip syncing process and start node right away
    pub skip_sync: bool,
    /// Optional sync checkpoint height
    pub checkpoint_height: Option<u32>,
    /// Optional sync checkpoint hash
    pub checkpoint: Option<String>,
}

/// Sync the node consensus state and start the corresponding task, based on node type.
pub async fn consensus_init_task(
    node: DarkfiNodePtr,
    config: ConsensusInitTaskConfig,
    ex: ExecutorPtr,
) -> Result<()> {
    // Check current canonical blockchain for curruption
    // TODO: create a restore method reverting each block backwards
    //       until its healthy again
    node.validator.consensus.healthcheck().await?;

    // Check if network genesis is in the future.
    let current = Timestamp::current_time().inner();
    let genesis = node.validator.consensus.module.read().await.genesis.inner();
    if current < genesis {
        let diff = genesis - current;
        info!(target: "darkfid::task::consensus_init_task", "Waiting for network genesis: {diff} seconds");
        sleep(diff).await;
    }

    // Generate a new fork to be able to extend
    info!(target: "darkfid::task::consensus_init_task", "Generating new empty fork...");
    node.validator.consensus.generate_empty_fork().await?;

    // Sync blockchain
    let comms_timeout = node.p2p_handler.p2p.settings().read().await.outbound_connect_timeout;
    let checkpoint = if !config.skip_sync {
        // Parse configured checkpoint
        if config.checkpoint_height.is_some() && config.checkpoint.is_none() {
            return Err(Error::ParseFailed("Blockchain configured checkpoint hash missing"))
        }

        let checkpoint = if let Some(height) = config.checkpoint_height {
            Some((height, HeaderHash::from_str(config.checkpoint.as_ref().unwrap())?))
        } else {
            None
        };

        loop {
            match sync_task(&node, checkpoint).await {
                Ok(_) => break,
                Err(e) => {
                    error!(target: "darkfid::task::consensus_task", "Sync task failed: {e}");
                    info!(target: "darkfid::task::consensus_task", "Sleeping for {comms_timeout} before retry...");
                    sleep(comms_timeout).await;
                }
            }
        }
        checkpoint
    } else {
        *node.validator.synced.write().await = true;
        None
    };

    // Gracefully handle network disconnections
    loop {
        match listen_to_network(&node, &ex).await {
            Ok(_) => return Ok(()),
            Err(Error::NetworkNotConnected) => {
                // Sync node again
                *node.validator.synced.write().await = false;
                node.validator.consensus.purge_forks().await?;
                if !config.skip_sync {
                    loop {
                        match sync_task(&node, checkpoint).await {
                            Ok(_) => break,
                            Err(e) => {
                                error!(target: "darkfid::task::consensus_task", "Sync task failed: {e}");
                                info!(target: "darkfid::task::consensus_task", "Sleeping for {comms_timeout} before retry...");
                                sleep(comms_timeout).await;
                            }
                        }
                    }
                } else {
                    *node.validator.synced.write().await = true;
                }
            }
            Err(e) => return Err(e),
        }
    }
}

/// Async task to start the consensus task, while monitoring for a network disconnections.
async fn listen_to_network(node: &DarkfiNodePtr, ex: &ExecutorPtr) -> Result<()> {
    // Grab proposals subscriber and subscribe to it
    let proposals_sub = node.subscribers.get("proposals").unwrap();
    let prop_subscription = proposals_sub.publisher.clone().subscribe().await;

    // Subscribe to the network disconnect subscriber
    let net_subscription = node.p2p_handler.p2p.hosts().subscribe_disconnect().await;

    let result = smol::future::or(
        monitor_network(&net_subscription),
        consensus_task(node, &prop_subscription, ex),
    )
    .await;

    // Terminate the subscriptions
    prop_subscription.unsubscribe().await;
    net_subscription.unsubscribe().await;

    result
}

/// Async task to monitor network disconnections.
async fn monitor_network(subscription: &Subscription<Error>) -> Result<()> {
    Err(subscription.receive().await)
}

/// Async task used for listening for new blocks and perform consensus.
async fn consensus_task(
    node: &DarkfiNodePtr,
    subscription: &Subscription<JsonNotification>,
    ex: &ExecutorPtr,
) -> Result<()> {
    info!(target: "darkfid::task::consensus_task", "Starting consensus task...");

    // Grab blocks subscriber
    let block_sub = node.subscribers.get("blocks").unwrap();

    // Create the garbage collection task using a dummy task
    let gc_task = StoppableTask::new();
    gc_task.clone().start(
        async { Ok(()) },
        |_| async { /* Do nothing */ },
        Error::GarbageCollectionTaskStopped,
        ex.clone(),
    );

    loop {
        subscription.receive().await;

        // Check if we can confirm anything and broadcast them
        let confirmed = match node.validator.confirmation().await {
            Ok(f) => f,
            Err(e) => {
                error!(
                    target: "darkfid::task::consensus_task",
                    "Confirmation failed: {e}"
                );
                continue
            }
        };

        if confirmed.is_empty() {
            continue
        }

        if let Err(e) = clean_blocktemplates(node).await {
            error!(target: "darkfid", "Failed cleaning mining block templates: {e}")
        }

        let mut notif_blocks = Vec::with_capacity(confirmed.len());
        for block in confirmed {
            notif_blocks.push(JsonValue::String(base64::encode(&serialize_async(&block).await)));
        }
        block_sub.notify(JsonValue::Array(notif_blocks)).await;

        // Invoke the detached garbage collection task
        gc_task.clone().stop().await;
        gc_task.clone().start(
            garbage_collect_task(node.clone()),
            |res| async {
                match res {
                    Ok(()) | Err(Error::GarbageCollectionTaskStopped) => { /* Do nothing */ }
                    Err(e) => {
                        error!(target: "darkfid", "Failed starting garbage collection task: {e}")
                    }
                }
            },
            Error::GarbageCollectionTaskStopped,
            ex.clone(),
        );
    }
}

/// Auxiliary function to drop mining block templates not referencing
/// active forks or last confirmed block.
async fn clean_blocktemplates(node: &DarkfiNodePtr) -> Result<()> {
    // Grab a lock over node mining templates
    let mut blocktemplates = node.blocktemplates.lock().await;
    let mut mm_blocktemplates = node.mm_blocktemplates.lock().await;

    // Early return if no mining block templates exist
    if blocktemplates.is_empty() && mm_blocktemplates.is_empty() {
        return Ok(())
    }

    // Grab a lock over node forks
    let forks = node.validator.consensus.forks.read().await;

    // Grab last confirmed block for checks
    let (_, last_confirmed) = node.validator.blockchain.last()?;

    // Loop through templates to find which can be dropped
    let mut dropped_templates = vec![];
    'outer: for (key, blocktemplate) in blocktemplates.iter() {
        // Loop through all the forks
        for fork in forks.iter() {
            // Traverse fork proposals sequence in reverse
            for p_hash in fork.proposals.iter().rev() {
                // Check if job extends this fork
                if &blocktemplate.block.header.previous == p_hash {
                    continue 'outer
                }
            }
        }

        // Check if it extends last confirmed block
        if blocktemplate.block.header.previous == last_confirmed {
            continue
        }

        // This job doesn't reference something so we drop it
        dropped_templates.push(key.clone());
    }

    // Drop jobs not referencing active forks or last confirmed block
    for key in dropped_templates {
        blocktemplates.remove(&key);
    }

    // Loop through merge mining templates to find which can be dropped
    let mut dropped_templates = vec![];
    'outer: for (key, (block, _, _)) in mm_blocktemplates.iter() {
        // Loop through all the forks
        for fork in forks.iter() {
            // Traverse fork proposals sequence in reverse
            for p_hash in fork.proposals.iter().rev() {
                // Check if job extends this fork
                if &block.header.previous == p_hash {
                    continue 'outer
                }
            }
        }

        // Check if it extends last confirmed block
        if block.header.previous == last_confirmed {
            continue
        }

        // This job doesn't reference something so we drop it
        dropped_templates.push(key.clone());
    }

    // Drop jobs not referencing active forks or last confirmed block
    for key in dropped_templates {
        mm_blocktemplates.remove(&key);
    }

    Ok(())
}
