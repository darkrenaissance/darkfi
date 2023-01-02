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

use crate::{
    consensus::{
        block::{BlockOrder, BlockResponse},
        state::{SlotCheckpointRequest, SlotCheckpointResponse},
        ValidatorStatePtr,
    },
    net, Result,
};
use log::{debug, info, warn};

/// async task used for block syncing.
pub async fn block_sync_task(p2p: net::P2pPtr, state: ValidatorStatePtr) -> Result<()> {
    info!(target: "consensus::block_sync", "Starting blockchain sync...");
    // Getting a random connected channel to ask from peers
    match p2p.clone().random_channel().await {
        Some(channel) => {
            // Communication setup for slot checkpoints
            let msg_subsystem = channel.get_message_subsystem();
            msg_subsystem.add_dispatch::<SlotCheckpointResponse>().await;
            let response_sub = channel.subscribe_msg::<SlotCheckpointResponse>().await?;

            // Node sends the last known slot checkpoint of the canonical blockchain
            // and loops until the response is the same slot (used to utilize batch requests).
            let mut last = state.read().await.blockchain.last_slot_checkpoint()?;
            info!(target: "consensus::block_sync", "Last known slot checkpoint: {:?}", last.slot);

            loop {
                // Node creates a `SlotCheckpointRequest` and sends it
                let request = SlotCheckpointRequest { slot: last.slot };
                channel.send(request).await?;

                // Node stores response data.
                let resp = response_sub.receive().await?;

                // Verify and store retrieved checkpoints
                debug!(target: "consensus::block_sync", "block_sync_task(): Processing received slot checkpoints");
                state.write().await.receive_slot_checkpoints(&resp.slot_checkpoints).await?;

                let last_received = state.read().await.blockchain.last_slot_checkpoint()?;
                info!(target: "consensus::block_sync", "Last received slot checkpoint: {:?}", last_received.slot);

                if last.slot == last_received.slot {
                    break
                }

                last = last_received;
            }

            // Communication setup for blocks
            let msg_subsystem = channel.get_message_subsystem();
            msg_subsystem.add_dispatch::<BlockResponse>().await;
            let response_sub = channel.subscribe_msg::<BlockResponse>().await?;

            // Node sends the last known block hash of the canonical blockchain
            // and loops until the response is the same block (used to utilize
            // batch requests).
            let mut last = state.read().await.blockchain.last()?;
            info!(target: "consensus::block_sync", "Last known block: {:?} - {:?}", last.0, last.1);

            loop {
                // Node creates a `BlockOrder` and sends it
                let order = BlockOrder { slot: last.0, block: last.1 };
                channel.send(order).await?;

                // Node stores response data.
                let resp = response_sub.receive().await?;

                // Verify and store retrieved blocks
                debug!(target: "consensus::block_sync", "block_sync_task(): Processing received blocks");
                state.write().await.receive_sync_blocks(&resp.blocks).await?;

                let last_received = state.read().await.blockchain.last()?;
                info!(target: "consensus::block_sync", "Last received block: {:?} - {:?}", last_received.0, last_received.1);

                if last == last_received {
                    break
                }

                last = last_received;
            }
        }
        None => warn!(target: "consensus::block_sync", "Node is not connected to other nodes"),
    };

    info!(target: "consensus::block_sync", "Blockchain synced!");
    Ok(())
}
