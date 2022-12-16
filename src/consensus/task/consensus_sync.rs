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

use log::{info, warn};

use crate::{
    consensus::{
        state::{
            ConsensusRequest, ConsensusResponse, ConsensusSlotCheckpointsRequest,
            ConsensusSlotCheckpointsResponse,
        },
        ValidatorStatePtr,
    },
    net::P2pPtr,
    Result,
};

/// async task used for consensus state syncing.
pub async fn consensus_sync_task(p2p: P2pPtr, state: ValidatorStatePtr) -> Result<()> {
    info!("Starting consensus state sync...");
    let channels_map = p2p.channels().lock().await;
    let values = channels_map.values();
    // Using len here because is_empty() uses unstable library feature
    // called 'exact_size_is_empty'.
    if values.len() != 0 {
        // Node iterates the channel peers to ask for their consensus state
        for channel in values {
            // Communication setup
            let msg_subsystem = channel.get_message_subsystem();
            msg_subsystem.add_dispatch::<ConsensusResponse>().await;
            let response_sub = channel.subscribe_msg::<ConsensusResponse>().await?;

            // Node creates a `ConsensusRequest` and sends it
            let request = ConsensusRequest {};
            channel.send(request).await?;

            // Node verifies response came from a participating node.
            // Extra validations can be added here.
            let response = response_sub.receive().await?;
            if response.nullifiers.is_empty() {
                warn!("Retrieved consensus state from a new node, retrying...");
                continue
            }

            // Node stores response data.
            let mut lock = state.write().await;
            lock.consensus.offset = response.offset;
            let mut forks = vec![];
            for fork in &response.forks {
                forks.push(fork.clone().into());
            }
            lock.consensus.forks = forks;
            lock.unconfirmed_txs = response.unconfirmed_txs.clone();
            lock.consensus.slot_checkpoints = response.slot_checkpoints.clone();
            lock.consensus.leaders_history = response.leaders_history.clone();
            lock.consensus.nullifiers = response.nullifiers.clone();

            break
        }
    } else {
        warn!("Node is not connected to other nodes");
    }

    info!("Consensus state synced!");
    Ok(())
}

/// async task used for consensus state syncing.
pub async fn consensus_sync_task2(p2p: P2pPtr, state: ValidatorStatePtr) -> Result<()> {
    info!("Starting consensus state sync...");
    // Loop through connected channels
    let channels_map = p2p.channels().lock().await;
    let values = channels_map.values();
    // Using len here because is_empty() uses unstable library feature
    // called 'exact_size_is_empty'.
    if values.len() == 0 {
        warn!("Node is not connected to other nodes");
        info!("Consensus state synced!");
        return Ok(())
    }

    // Node iterates the channel peers to check if at least on peer has seen slot checkpoints
    let mut peer = None;
    for channel in values {
        // Communication setup
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<ConsensusSlotCheckpointsResponse>().await;
        let response_sub = channel.subscribe_msg::<ConsensusSlotCheckpointsResponse>().await?;

        // Node creates a `ConsensusSlotCheckpointsRequest` and sends it
        let request = ConsensusSlotCheckpointsRequest {};
        channel.send(request).await?;

        // Node checks response
        let response = response_sub.receive().await?;
        if !response.slot_checkpoints {
            warn!("Node has not seen any slot checkpoints, retrying...");
            continue
        }

        // Keep peer to ask for consensus state
        peer = Some(channel.clone());
        break
    }

    // Release channels lock
    drop(channels_map);

    // If no peer knows about any slot checkpoints, that means that the network was bootstrapped or restarted
    // and no node has started consensus.
    if peer.is_none() {
        warn!("No node that has seen any slot checkpoints was found.");
        info!("Consensus state synced!");
        return Ok(())
    }
    let peer = peer.unwrap();

    // Listen for next finalization
    info!("Waiting for next finalization...");
    let subscriber = state.read().await.subscribers.get("blocks").unwrap().clone();
    let subscription = subscriber.subscribe().await;
    subscription.receive().await;

    // After finalization occurs, sync our consensus state.
    // This ensures that the received state always consists of 1 fork with one proposal.
    info!("Finalization signal received, requesting consensus state...");
    // Communication setup
    let msg_subsystem = peer.get_message_subsystem();
    msg_subsystem.add_dispatch::<ConsensusResponse>().await;
    let response_sub = peer.subscribe_msg::<ConsensusResponse>().await?;

    // Node creates a `ConsensusRequest` and sends it
    let request = ConsensusRequest {};
    peer.send(request).await?;

    // Node verifies response came from a participating node.
    // Extra validations can be added here.
    let response = response_sub.receive().await?;

    // Node stores response data.
    let mut lock = state.write().await;
    lock.consensus.offset = response.offset;
    let mut forks = vec![];
    for fork in &response.forks {
        forks.push(fork.clone().into());
    }
    lock.consensus.forks = forks;
    lock.unconfirmed_txs = response.unconfirmed_txs.clone();
    lock.consensus.slot_checkpoints = response.slot_checkpoints.clone();
    lock.consensus.leaders_history = response.leaders_history.clone();
    lock.consensus.nullifiers = response.nullifiers.clone();

    info!("Consensus state synced!");
    Ok(())
}
