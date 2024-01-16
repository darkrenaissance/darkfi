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

use crate::{
    consensus::{
        block::{BlockOrder, BlockResponse},
        state::{SlotRequest, SlotResponse},
        ValidatorStatePtr,
    },
    net, Result,
};
use log::{debug, info, warn};

/// async task used for block syncing.
pub async fn block_sync_task(p2p: net::P2pPtr, state: ValidatorStatePtr) -> Result<()> {
    info!(target: "consensus::block_sync", "Starting blockchain sync...");
    // Getting a random connected channel to ask from peers
    match p2p.random_channel().await {
        Some(channel) => {
            let msg_subsystem = channel.message_subsystem();

            // Communication setup for slots
            msg_subsystem.add_dispatch::<SlotResponse>().await;
            let slot_response_sub = channel.subscribe_msg::<SlotResponse>().await?;

            // Communication setup for blocks
            msg_subsystem.add_dispatch::<BlockResponse>().await;
            let block_response_sub = channel.subscribe_msg::<BlockResponse>().await?;

            // Node loops until both slots and blocks have been synced
            let mut slots_synced = false;
            let mut blocks_synced = false;
            loop {
                // Node sends the last known slot of the canonical blockchain
                // and loops until the response is the same slot (used to utilize batch requests).
                let mut last = state.read().await.blockchain.last_slot()?;
                info!(target: "consensus::block_sync", "Last known slot: {:?}", last.id);

                loop {
                    // Node creates a `SlotRequest` and sends it
                    let request = SlotRequest { slot: last.id };
                    channel.send(&request).await?;

                    // Node stores response data.
                    let resp = slot_response_sub.receive().await?;

                    // Verify and store retrieveds
                    debug!(target: "consensus::block_sync", "block_sync_task(): Processing received slots");
                    state.write().await.receive_slots(&resp.slots).await?;

                    let last_received = state.read().await.blockchain.last_slot()?;
                    info!(target: "consensus::block_sync", "Last received slot: {:?}", last_received.id);

                    if last.id == last_received.id {
                        break
                    }

                    blocks_synced = false;
                    last = last_received;
                }

                // We force a recheck of slots after blocks have been synced
                if blocks_synced {
                    slots_synced = true;
                }

                // Node sends the last known block hash of the canonical blockchain
                // and loops until the response is the same block (used to utilize
                // batch requests).
                let mut last = state.read().await.blockchain.last()?;
                info!(target: "consensus::block_sync", "Last known block: {:?} - {:?}", last.0, last.1);

                loop {
                    // Node creates a `BlockOrder` and sends it
                    let order = BlockOrder { slot: last.0, block: last.1 };
                    channel.send(&order).await?;

                    // Node stores response data.
                    let _resp = block_response_sub.receive().await?;

                    // Verify and store retrieved blocks
                    debug!(target: "consensus::block_sync", "block_sync_task(): Processing received blocks");
                    //state.write().await.receive_sync_blocks(&resp.blocks).await?;

                    let last_received = state.read().await.blockchain.last()?;
                    info!(target: "consensus::block_sync", "Last received block: {:?} - {:?}", last_received.0, last_received.1);

                    if last == last_received {
                        blocks_synced = true;
                        break
                    }

                    slots_synced = false;
                    last = last_received;
                }

                if slots_synced && blocks_synced {
                    break
                }
            }
        }
        None => warn!(target: "consensus::block_sync", "Node is not connected to other nodes"),
    };

    state.write().await.synced = true;
    info!(target: "consensus::block_sync", "Blockchain synced!");
    Ok(())
}
