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

use log::{info, warn};

use crate::{
    consensus::{
        state::{ConsensusRequest, ConsensusResponse, ConsensusSyncRequest, ConsensusSyncResponse},
        Float10, ValidatorStatePtr,
    },
    net::P2pPtr,
    util::async_util::sleep,
    Result,
};

/// async task used for consensus state syncing.
/// Returns flag if node is not connected to other peers or consensus hasn't started,
/// so it can immediately start proposing proposals.
pub async fn consensus_sync_task(p2p: P2pPtr, state: ValidatorStatePtr) -> Result<bool> {
    info!(target: "consensus::consensus_sync", "Starting consensus state sync...");
    let current_slot = state.read().await.consensus.time_keeper.current_slot();
    // Loop through connected channels
    let channels_map = p2p.channels().lock().await;
    let values = channels_map.values();
    // Using len here because is_empty() uses unstable library feature
    // called 'exact_size_is_empty'.
    if values.len() == 0 {
        warn!(target: "consensus::consensus_sync", "Node is not connected to other nodes");
        let mut lock = state.write().await;
        lock.consensus.bootstrap_slot = current_slot;
        lock.consensus.init_coins().await?;
        info!(target: "consensus::consensus_sync", "Consensus state synced!");
        return Ok(true)
    }

    // Node iterates the channel peers to check if at least on peer has seen slots
    let mut peer = None;
    for channel in values {
        // Communication setup
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<ConsensusSyncResponse>().await;
        let response_sub = channel.subscribe_msg::<ConsensusSyncResponse>().await?;
        // Node creates a `ConsensusSyncRequest` and sends it
        let request = ConsensusSyncRequest {};
        channel.send(request).await?;
        // Node checks response
        let response = response_sub.receive().await?;
        if response.bootstrap_slot == current_slot {
            warn!(target: "consensus::consensus_sync", "Network was just bootstraped, checking rest nodes");
            continue
        }
        if !response.proposing {
            warn!(target: "consensus::consensus_sync", "Node is not proposing, checking rest nodes");
            continue
        }
        if response.is_empty {
            warn!(target: "consensus::consensus_sync", "Node has not seen any slots, retrying...");
            continue
        }
        // Keep peer to ask for consensus state
        peer = Some(channel.clone());
        break
    }

    // Release channels lock
    drop(channels_map);

    // If no peer knows about any slots, that means that the network was bootstrapped or restarted
    // and no node has started consensus.
    if peer.is_none() {
        warn!(target: "consensus::consensus_sync", "No node that has seen any slots was found, or network was just boostrapped.");
        let mut lock = state.write().await;
        lock.consensus.bootstrap_slot = current_slot;
        lock.consensus.init_coins().await?;
        info!(target: "consensus::consensus_sync", "Consensus state synced!");
        return Ok(true)
    }
    let peer = peer.unwrap();

    // Listen for next finalization
    info!(target: "consensus::consensus_sync", "Waiting for next finalization...");
    let subscriber = state.read().await.subscribers.get("blocks").unwrap().clone();
    let subscription = subscriber.subscribe().await;
    subscription.receive().await;
    subscription.unsubscribe().await;

    // After finalization occurs, sync our consensus state.
    // This ensures that the received state always consists of 1 fork with one proposal.
    info!(target: "consensus::consensus_sync", "Finalization signal received, requesting consensus state...");
    // Communication setup
    let msg_subsystem = peer.get_message_subsystem();
    msg_subsystem.add_dispatch::<ConsensusResponse>().await;
    let response_sub = peer.subscribe_msg::<ConsensusResponse>().await?;
    // Node creates a `ConsensusRequest` and sends it
    peer.send(ConsensusRequest {}).await?;

    // Node verifies response came from a participating node.
    // Extra validations can be added here.
    let mut response = response_sub.receive().await?;
    // Verify that peer has finished finalizing forks
    loop {
        if !response.forks.is_empty() {
            warn!(target: "consensus::consensus_sync", "Peer has not finished finalization, retrying...");
            sleep(1).await;
            peer.send(ConsensusRequest {}).await?;
            response = response_sub.receive().await?;
            continue
        }
        break
    }

    // Verify that the node has received all finalized blocks
    loop {
        if !state.read().await.blockchain.has_slot_order(response.current_slot)? {
            warn!(target: "consensus::consensus_sync", "Node has not finished finalization, retrying...");
            sleep(1).await;
            continue
        }
        break
    }

    // Node stores response data.
    let mut lock = state.write().await;
    let mut forks = vec![];
    for fork in &response.forks {
        forks.push(fork.clone().into());
    }
    lock.consensus.bootstrap_slot = response.bootstrap_slot;
    lock.consensus.forks = forks;
    lock.append_pending_txs(&response.pending_txs).await;
    lock.consensus.slots = response.slots.clone();
    lock.consensus.previous_leaders = 1;
    let mut f_history = vec![];
    for f in &response.f_history {
        let f_float = Float10::try_from(f.as_str()).unwrap();
        f_history.push(f_float);
    }
    lock.consensus.f_history = f_history;
    let mut err_history = vec![];
    for err in &response.err_history {
        let err_float = Float10::try_from(err.as_str()).unwrap();
        err_history.push(err_float);
    }
    lock.consensus.err_history = err_history;
    lock.consensus.nullifiers = response.nullifiers.clone();
    lock.consensus.init_coins().await?;

    info!(target: "consensus::consensus_sync", "Consensus state synced!");
    Ok(false)
}
