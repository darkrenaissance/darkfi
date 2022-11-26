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

use std::time::Duration;

use log::{debug, error, info};

use super::consensus_sync_task;
use crate::{consensus::ValidatorStatePtr, net::P2pPtr, util::async_util::sleep};

/// async task used for participating in the consensus protocol
pub async fn proposal_task(consensus_p2p: P2pPtr, sync_p2p: P2pPtr, state: ValidatorStatePtr) {
    // Node waits just before the current or next epoch last finalization syncing period, so it can
    // start syncing latest state.
    let mut seconds_until_next_epoch = state.read().await.next_n_epoch_start(1);
    let three_secs = Duration::new(3, 0);

    loop {
        if seconds_until_next_epoch > three_secs {
            seconds_until_next_epoch -= three_secs;
            break
        }

        info!("consensus: Waiting for next epoch ({:?} sec)", seconds_until_next_epoch);
        sleep(seconds_until_next_epoch.as_secs()).await;
        seconds_until_next_epoch = state.read().await.next_n_epoch_start(1);
    }

    info!("consensus: Waiting for next epoch ({:?} sec)", seconds_until_next_epoch);
    sleep(seconds_until_next_epoch.as_secs()).await;

    // Node syncs its consensus state
    if let Err(e) = consensus_sync_task(consensus_p2p.clone(), state.clone()).await {
        error!("consensus: Failed syncing consensus state: {}. Quitting consensus.", e);
        // TODO: Perhaps notify over a channel in order to
        // stop consensus p2p protocols.
        return
    };

    // Node modifies its participating slot to next.
    match state.write().await.set_participating() {
        Ok(()) => info!("consensus: Node will start participating in the next slot"),
        Err(e) => error!("consensus: Failed to set participation slot: {}", e),
    }

    loop {
        // Node sleeps until finalization sync period start (2 seconds before next slot)
        let seconds_sync_period =
            (state.read().await.next_n_slot_start(1) - Duration::new(2, 0)).as_secs();
        info!("consensus: Waiting for finalization sync period ({} sec)", seconds_sync_period);
        sleep(seconds_sync_period).await;

        // Check if any forks can be finalized
        match state.write().await.chain_finalization().await {
            Ok(to_broadcast) => {
                // Broadcast finalized blocks info, if any:
                if to_broadcast.len() > 0 {
                    info!("consensus: Broadcasting finalized blocks");
                    for info in to_broadcast {
                        match sync_p2p.broadcast(info).await {
                            Ok(()) => info!("consensus: Broadcasted block"),
                            Err(e) => error!("consensus: Failed broadcasting block: {}", e),
                        }
                    }
                } else {
                    info!("consensus: No finalized blocks to broadcast");
                }
            }
            Err(e) => {
                error!("consensus: Finalization check failed: {}", e);
            }
        }

        // Node sleeps until next slot
        let seconds_next_slot = state.read().await.next_n_slot_start(1).as_secs();
        info!("consensus: Waiting for next slot ({} sec)", seconds_next_slot);
        sleep(seconds_next_slot).await;

        // Node checks if epoch has changed, to generate new epoch coins
        let epoch_changed = state.write().await.epoch_changed().await;
        match epoch_changed {
            Ok(changed) => {
                if changed {
                    info!("consensus: New epoch started: {}", state.read().await.current_epoch());
                    let mut coins = vec![];
                    for slot_coins in &state.read().await.consensus.coins {
                        let mut slot_coins_inputs = vec![];
                        for slot_coin in slot_coins {
                            slot_coins_inputs.push(slot_coin.public_inputs());
                        }
                        coins.push(slot_coins_inputs);
                    }
                }
            }
            Err(e) => {
                error!("consensus: Epoch check failed: {}", e);
                continue
            }
        };

        // Node checks if it's the slot leader to generate a new proposal
        // for that slot.
        let (won, idx, sigma1, sigma2) = state.write().await.is_slot_leader();
        let result = if won { state.write().await.propose(idx, sigma1, sigma2) } else { Ok(None) };
        let proposal = match result {
            Ok(prop) => {
                if prop.is_none() {
                    info!("consensus: Node is not the slot lead");
                    continue
                }
                prop.unwrap()
            }
            Err(e) => {
                error!("consensus: Block proposal failed: {}", e);
                continue
            }
        };

        // Node stores the proposal and broadcast to rest nodes
        info!("consensus: Node is the slot leader: Proposed block: {}", proposal);
        debug!("consensus: Full proposal: {:?}", proposal);
        match state.write().await.receive_proposal(&proposal).await {
            Ok(()) => {
                info!("consensus: Block proposal saved successfully");
                // Broadcast proposal to other consensus nodes
                match consensus_p2p.broadcast(proposal).await {
                    Ok(()) => info!("consensus: Proposal broadcasted successfully"),
                    Err(e) => error!("consensus: Failed broadcasting proposal: {}", e),
                }
            }
            Err(e) => {
                error!("consensus: Block proposal save failed: {}", e);
            }
        }
    }
}
