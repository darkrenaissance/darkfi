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

use std::{str::FromStr, sync::Arc};

use darkfi::{
    blockchain::HeaderHash,
    rpc::util::JsonValue,
    system::{sleep, StoppableTask},
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
    Darkfid,
};

/// Auxiliary structure representing node consensus init task configuration
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
    node: Arc<Darkfid>,
    config: ConsensusInitTaskConfig,
    ex: Arc<smol::Executor<'static>>,
) -> Result<()> {
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

        let spend_hook = match config.spend_hook {
            Some(s) => match FuncId::from_str(&s) {
                Ok(s) => Some(s),
                Err(_) => return Err(Error::ParseFailed("Invalid spend hook")),
            },
            None => None,
        };

        let user_data = match config.user_data {
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
            miner_task(
                node.clone(),
                recipient_config.as_ref().unwrap(),
                config.skip_sync,
                ex.clone(),
            )
            .await
        } else {
            replicator_task(node.clone(), ex.clone()).await
        };

        match result {
            Ok(_) => return Ok(()),
            Err(Error::NetworkOperationFailed) => {
                // Sync node again
                *node.validator.synced.write().await = false;
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
async fn replicator_task(node: Arc<Darkfid>, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    smol::future::or(monitor_network(node.clone()), consensus_task(node, ex)).await
}

/// Async task to monitor network disconnections.
async fn monitor_network(node: Arc<Darkfid>) -> Result<()> {
    loop {
        // Check if we are connected to the network
        if node.p2p.hosts().channels().await.is_empty() {
            error!(target: "darkfid::task::consensus::monitor_network", "Node disconnected from the network");
            return Err(Error::NetworkOperationFailed)
        }

        sleep(node.p2p.settings().outbound_connect_timeout).await;
    }
}

/// Async task used for listening for new blocks and perform consensus.
async fn consensus_task(node: Arc<Darkfid>, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    info!(target: "darkfid::task::consensus_task", "Starting consensus task...");

    // Grab blocks subscriber
    let block_sub = node.subscribers.get("blocks").unwrap();

    // Grab proposals subscriber and subscribe to it
    let proposals_sub = node.subscribers.get("proposals").unwrap();
    let subscription = proposals_sub.publisher.clone().subscribe().await;

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

        // Check if we can finalize anything and broadcast them
        let finalized = match node.validator.finalization().await {
            Ok(f) => f,
            Err(e) => {
                error!(
                    target: "darkfid::task::consensus_task",
                    "Finalization failed: {e}"
                );
                continue
            }
        };
        if !finalized.is_empty() {
            let mut notif_blocks = Vec::with_capacity(finalized.len());
            for block in finalized {
                notif_blocks
                    .push(JsonValue::String(base64::encode(&serialize_async(&block).await)));
            }
            block_sub.notify(JsonValue::Array(notif_blocks)).await;

            // Invoke the detached garbage collection task
            gc_task.clone().stop().await;
            gc_task.clone().start(
                garbage_collect_task(node.clone()),
                |res| async {
                    match res {
                        Ok(()) | Err(Error::GarbageCollectionTaskStopped) => { /* Do nothing */ }
                        Err(e) => error!(target: "darkfid", "Failed starting garbage collection task: {}", e),
                    }
                },
                Error::GarbageCollectionTaskStopped,
                ex.clone(),
            );
        }
    }
}
