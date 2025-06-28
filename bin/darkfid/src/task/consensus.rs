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
use darkfi_sdk::{
    crypto::{FuncId, PublicKey},
    pasta::{group::ff::PrimeField, pallas},
};
use darkfi_serial::serialize_async;
use log::{error, info};

use crate::{
    task::{garbage_collect_task, miner::MinerRewardsRecipientConfig, miner_task, sync_task},
    DarkfiNodePtr,
};

/// Auxiliary structure representing node consensus init task configuration
#[derive(Clone)]
pub struct ConsensusInitTaskConfig {
    pub skip_sync: bool,
    pub checkpoint_height: Option<u32>,
    pub checkpoint: Option<String>,
    pub miner: bool,
    pub recipient: Option<String>,
    pub spend_hook: Option<String>,
    pub user_data: Option<String>,
    pub bootstrap: u64,
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

    // Check if network is configured to start in the future.
    // NOTE: Always configure the network to start in the future when bootstrapping
    // or restarting it.
    let current = Timestamp::current_time().inner();
    if current < config.bootstrap {
        let diff = config.bootstrap - current;
        info!(target: "darkfid::task::consensus_init_task", "Waiting for network bootstrap: {diff} seconds");
        sleep(diff).await;
    }

    // Generate a new fork to be able to extend
    info!(target: "darkfid::task::consensus_init_task", "Generating new empty fork...");
    node.validator.consensus.generate_empty_fork().await?;

    // Sync blockchain
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

        sync_task(&node, checkpoint).await?;
        checkpoint
    } else {
        *node.validator.synced.write().await = true;
        None
    };

    // Grab rewards recipient public key(address) if node is a miner,
    // along with configured spend hook and user data.
    let recipient_config = if config.miner {
        if config.recipient.is_none() {
            return Err(Error::ParseFailed("Recipient address missing"))
        }
        let recipient = match PublicKey::from_str(config.recipient.as_ref().unwrap()) {
            Ok(address) => address,
            Err(_) => return Err(Error::InvalidAddress),
        };

        let spend_hook = match &config.spend_hook {
            Some(s) => match FuncId::from_str(s) {
                Ok(s) => Some(s),
                Err(_) => return Err(Error::ParseFailed("Invalid spend hook")),
            },
            None => None,
        };

        let user_data = match &config.user_data {
            Some(u) => {
                let bytes: [u8; 32] = match bs58::decode(&u).into_vec()?.try_into() {
                    Ok(b) => b,
                    Err(_) => return Err(Error::ParseFailed("Invalid user data")),
                };

                match pallas::Base::from_repr(bytes).into() {
                    Some(v) => Some(v),
                    None => return Err(Error::ParseFailed("Invalid user data")),
                }
            }
            None => None,
        };

        Some(MinerRewardsRecipientConfig { recipient, spend_hook, user_data })
    } else {
        None
    };

    // Gracefully handle network disconnections
    loop {
        let result = if config.miner {
            miner_task(&node, recipient_config.as_ref().unwrap(), config.skip_sync, &ex).await
        } else {
            replicator_task(&node, &ex).await
        };

        match result {
            Ok(_) => return Ok(()),
            Err(Error::NetworkNotConnected) => {
                // Sync node again
                *node.validator.synced.write().await = false;
                node.validator.consensus.purge_forks().await?;
                if !config.skip_sync {
                    sync_task(&node, checkpoint).await?;
                } else {
                    *node.validator.synced.write().await = true;
                }
            }
            Err(e) => return Err(e),
        }
    }
}

/// Async task to start the consensus task, while monitoring for a network disconnections.
async fn replicator_task(node: &DarkfiNodePtr, ex: &ExecutorPtr) -> Result<()> {
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
