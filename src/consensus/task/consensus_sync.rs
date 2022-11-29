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
        state::{ConsensusRequest, ConsensusResponse},
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
            if response.leaders_nullifiers.is_empty() {
                warn!("Retrieved consensus state from a new node, retrying...");
                continue
            }

            // Node stores response data.
            let mut lock = state.write().await;
            lock.consensus.offset = response.offset;
            lock.consensus.proposals = response.proposals.clone();
            lock.unconfirmed_txs = response.unconfirmed_txs.clone();
            lock.consensus.slot_checkpoints = response.slot_checkpoints.clone();
            lock.consensus.leaders_nullifiers = response.leaders_nullifiers.clone();
            lock.consensus.leaders_spent_coins = response.leaders_spent_coins.clone();

            break
        }
    } else {
        warn!("Node is not connected to other nodes");
    }

    info!("Consensus state synced!");
    Ok(())
}
