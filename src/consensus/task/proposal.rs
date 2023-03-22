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

use std::time::Duration;

use async_std::sync::Arc;
use log::{debug, error, info, warn};

use super::consensus_sync_task;
use crate::{
    consensus::{constants, ValidatorStatePtr},
    net::P2pPtr,
    util::{async_util::sleep, time::Timestamp},
};

/// async task used for participating in the consensus protocol
pub async fn proposal_task(
    consensus_p2p: P2pPtr,
    sync_p2p: P2pPtr,
    state: ValidatorStatePtr,
    ex: Arc<smol::Executor<'_>>,
) {
    // Check if network is configured to start in the future,
    // otherwise wait for current or next slot finalization period for optimal sync conditions.
    // NOTE: Network beign configured to start in the future should always be the case
    // when bootstrapping or restarting a network.
    let current_ts = Timestamp::current_time();
    let bootstrap_ts = state.read().await.consensus.bootstrap_ts;
    if current_ts < bootstrap_ts {
        let diff = bootstrap_ts.0 - current_ts.0;
        info!(target: "consensus::proposal", "consensus: Waiting for network bootstrap: {} seconds", diff);
        sleep(diff as u64).await;
    } else {
        let mut sleep_time = state.read().await.consensus.next_n_slot_start(1);
        let sync_offset = Duration::new(constants::FINAL_SYNC_DUR, 0);
        loop {
            if sleep_time > sync_offset {
                sleep_time -= sync_offset;
                break
            }
            info!(target: "consensus::proposal", "consensus: Waiting for next slot ({:?})", sleep_time);
            sleep(sleep_time.as_secs()).await;
            sleep_time = state.read().await.consensus.next_n_slot_start(1);
        }
        info!(target: "consensus::proposal", "consensus: Waiting for finalization sync period ({:?})", sleep_time);
        sleep(sleep_time.as_secs()).await;
    }

    let mut retries = 0;
    // Sync loop
    loop {
        // Resetting consensus state, so node can still follow the finalized blocks by
        // the sync p2p network/protocols
        state.write().await.consensus.reset();

        // Checking sync retries
        if retries > constants::SYNC_MAX_RETRIES {
            error!(target: "consensus::proposal", "consensus: Node reached max sync retries ({}) due to not being able to follow up with consensus processing.", constants::SYNC_MAX_RETRIES);
            warn!(target: "consensus::proposal", "consensus: Terminating consensus participation.");
            break
        }

        // Node syncs its consensus state
        match consensus_sync_task(consensus_p2p.clone(), state.clone()).await {
            Ok(p) => {
                // Check if node is not connected to other nodes and can
                // start proposing immediately.
                if p {
                    info!(target: "consensus::proposal", "consensus: Node can start proposing!");
                    state.write().await.consensus.proposing = p;
                }
            }
            Err(e) => {
                error!(target: "consensus::proposal", "consensus: Failed syncing consensus state: {}. Quitting consensus.", e);
                // TODO: Perhaps notify over a channel in order to
                // stop consensus p2p protocols.
                return
            }
        };

        // Node modifies its participating slot to next.
        match state.write().await.consensus.set_participating() {
            Ok(()) => {
                info!(target: "consensus::proposal", "consensus: Node will start participating in the next slot")
            }
            Err(e) => {
                error!(target: "consensus::proposal", "consensus: Failed to set participation slot: {}", e)
            }
        }

        // Record epoch we start the consensus loop
        let start_epoch = state.read().await.consensus.current_epoch();

        // Start executing consensus
        consensus_loop(consensus_p2p.clone(), sync_p2p.clone(), state.clone(), ex.clone()).await;

        // Reset retries counter if more epochs have passed than sync retries duration
        let break_epoch = state.read().await.consensus.current_epoch();
        if (break_epoch - start_epoch) > constants::SYNC_RETRIES_DURATION {
            retries = 0;
        }

        // Increase retries count on consensus loop break
        retries += 1;
    }
}

/// Consensus protocol loop
async fn consensus_loop(
    consensus_p2p: P2pPtr,
    sync_p2p: P2pPtr,
    state: ValidatorStatePtr,
    ex: Arc<smol::Executor<'_>>,
) {
    // Note: when a node can start produce proposals is only enforced in code,
    // where we verify if the hardware can keep up with the consensus, by
    // counting how many consecutive slots node successfully listened and process
    // everything. Additionally, we check each proposer coin creation slot to be
    // greater than an epoch length. Later, this will be enforced via contract,
    // where it will be explicit when a node can produce proposals,
    // and after which slot they can be considered as valid.
    let mut listened_slots = 0;
    let mut changed_status = false;
    loop {
        // Check if node can start proposing.
        // This code ensures that we only change the status once
        // and listened_slots doesn't increment further.
        if listened_slots > constants::EPOCH_LENGTH {
            if !changed_status {
                info!(target: "consensus::proposal", "consensus: Node can start proposing!");
                state.write().await.consensus.proposing = true;
                changed_status = true;
            }
        } else {
            listened_slots += 1;
        }

        // Node waits and execute consensus protocol propose period.
        if propose_period(consensus_p2p.clone(), state.clone()).await {
            // Node needs to resync
            warn!(
                target: "consensus::proposal",
                "consensus: Node missed slot {} due to proposal processing, resyncing...",
                state.read().await.consensus.current_slot()
            );
            break
        }

        // Node waits and execute consensus protocol finalization period.
        if finalization_period(sync_p2p.clone(), state.clone(), ex.clone()).await {
            // Node needs to resync
            warn!(
                target: "consensus::proposal",
                "consensus: Node missed slot {} due to finalizated blocks processing, resyncing...",
                state.read().await.consensus.current_slot()
            );
            break
        }
    }
}

/// async function to wait and execute consensus protocol propose period.
/// Propose period consists of 2 parts:
///     - Generate slot sigmas and checkpoint
///     - Check if slot leader to generate and broadcast proposal
/// Returns flag in case node needs to resync.
async fn propose_period(consensus_p2p: P2pPtr, state: ValidatorStatePtr) -> bool {
    // Node sleeps until next slot
    let seconds_next_slot = state.read().await.consensus.next_n_slot_start(1).as_secs();
    info!(target: "consensus::proposal", "consensus: Waiting for next slot ({} sec)", seconds_next_slot);
    sleep(seconds_next_slot).await;

    // Keep a record of slot to verify if next slot got skipped during processing
    let processing_slot = state.read().await.consensus.current_slot();

    // Retrieve slot sigmas
    let (sigma1, sigma2) = state.write().await.consensus.sigmas();
    // Node checks if epoch has changed and generate slot checkpoint
    let epoch_changed = state.write().await.consensus.epoch_changed(sigma1, sigma2).await;
    match epoch_changed {
        Ok(changed) => {
            if changed {
                info!(target: "consensus::proposal", "consensus: New epoch started: {}", state.read().await.consensus.epoch);
            }
        }
        Err(e) => {
            error!(target: "consensus::proposal", "consensus: Epoch check failed: {}", e);
            return false
        }
    };

    // Node checks if it's the slot leader to generate a new proposal
    // for that slot.
    let (won, fork_index, coin_index) =
        state.write().await.consensus.is_slot_leader(sigma1, sigma2);
    let result = if won {
        state.write().await.propose(processing_slot, fork_index, coin_index, sigma1, sigma2).await
    } else {
        Ok(None)
    };
    let (proposal, coin, derived_blind) = match result {
        Ok(pair) => {
            if pair.is_none() {
                info!(target: "consensus::proposal", "consensus: Node is not the slot lead");
                return false
            }
            pair.unwrap()
        }
        Err(e) => {
            error!(target: "consensus::proposal", "consensus: Block proposal failed: {}", e);
            return false
        }
    };

    // Node checks if it missed finalization period due to proposal creation
    let next_slot_start = state.read().await.consensus.next_n_slot_start(1);
    if next_slot_start.as_secs() <= constants::FINAL_SYNC_DUR {
        warn!(
            target: "consensus::proposal",
            "consensus: Node missed slot {} finalization period due to proposal creation, resyncing...",
            state.read().await.consensus.current_slot()
        );
        return true
    }

    // Node stores the proposal and broadcast to rest nodes

    info!(target: "consensus::proposal", "consensus: Node is the slot leader: Proposed block: {}", proposal);
    debug!(target: "consensus::proposal", "consensus: Full proposal: {:?}", proposal);
    match state
        .write()
        .await
        .receive_proposal(&proposal, Some((coin_index, coin, derived_blind)))
        .await
    {
        Ok(_) => {
            // Here we don't have to check to broadcast, because the flag
            // will always be true, since the node is able to produce proposals
            info!(target: "consensus::proposal", "consensus: Block proposal saved successfully");
            // Broadcast proposal to other consensus nodes
            match consensus_p2p.broadcast(proposal).await {
                Ok(()) => {
                    info!(target: "consensus::proposal", "consensus: Proposal broadcasted successfully")
                }
                Err(e) => {
                    error!(target: "consensus::proposal", "consensus: Failed broadcasting proposal: {}", e)
                }
            }
        }
        Err(e) => {
            error!(target: "consensus::proposal", "consensus: Block proposal save failed: {}", e);
        }
    }

    // Verify node didn't skip next slot
    processing_slot != state.read().await.consensus.current_slot()
}

/// async function to wait and execute consensus protocol finalization period.
/// Returns flag in case node needs to resync.
async fn finalization_period(
    sync_p2p: P2pPtr,
    state: ValidatorStatePtr,
    ex: Arc<smol::Executor<'_>>,
) -> bool {
    // Node sleeps until finalization sync period starts
    let next_slot_start = state.read().await.consensus.next_n_slot_start(1);
    if next_slot_start.as_secs() > constants::FINAL_SYNC_DUR {
        let seconds_sync_period =
            (next_slot_start - Duration::new(constants::FINAL_SYNC_DUR, 0)).as_secs();
        info!(target: "consensus::proposal", "consensus: Waiting for finalization sync period ({} sec)", seconds_sync_period);
        sleep(seconds_sync_period).await;
    } else {
        warn!(
            target: "consensus::proposal",
            "consensus: Node missed slot {} finalization period due to proposals processing, resyncing...",
            state.read().await.consensus.current_slot()
        );
        return true
    }

    // Keep a record of slot to verify if next slot got skipped during processing
    let completed_slot = state.read().await.consensus.current_slot();

    // Check if any forks can be finalized
    match state.write().await.chain_finalization().await {
        Ok((to_broadcast_block, to_broadcast_slot_checkpoints)) => {
            // Broadcasting in background
            if !to_broadcast_block.is_empty() || !to_broadcast_slot_checkpoints.is_empty() {
                ex.spawn(async move {
                    // Broadcast finalized blocks info, if any:
                    info!(target: "consensus::proposal", "consensus: Broadcasting finalized blocks");
                    for info in to_broadcast_block {
                        match sync_p2p.broadcast(info).await {
                            Ok(()) => info!(target: "consensus::proposal", "consensus: Broadcasted block"),
                            Err(e) => error!(target: "consensus::proposal", "consensus: Failed broadcasting block: {}", e),
                        }
                    }

                    // Broadcast finalized slot checkpoints, if any:
                    info!(target: "consensus::proposal", "consensus: Broadcasting finalized slot checkpoints");
                    for slot_checkpoint in to_broadcast_slot_checkpoints {
                        match sync_p2p.broadcast(slot_checkpoint).await {
                            Ok(()) => info!(target: "consensus::proposal", "consensus: Broadcasted slot_checkpoint"),
                            Err(e) => {
                                error!(target: "consensus::proposal", "consensus: Failed broadcasting slot_checkpoint: {}", e)
                            }
                        }
                    }
                })
                .detach();
            } else {
                info!(target: "consensus::proposal", "consensus: No finalized blocks or slot checkpoints to broadcast");
            }
        }
        Err(e) => {
            error!(target: "consensus::proposal", "consensus: Finalization check failed: {}", e);
        }
    }

    // Verify node didn't skip next slot
    completed_slot != state.read().await.consensus.current_slot()
}
