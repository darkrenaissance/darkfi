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

use darkfi::{
    validator::proto::{SyncRequest, SyncResponse},
    Result,
};
use log::{debug, info, warn};

use crate::Darkfid;

/// async task used for block syncing
pub async fn sync_task(node: &Darkfid) -> Result<()> {
    info!(target: "darkfid::task::sync_task", "Starting blockchain sync...");
    // Getting a random connected channel to ask from peers
    match node.sync_p2p.random_channel().await {
        Some(channel) => {
            // Communication setup
            let msg_subsystem = channel.message_subsystem();
            msg_subsystem.add_dispatch::<SyncResponse>().await;
            let block_response_sub = channel.subscribe_msg::<SyncResponse>().await?;

            // Node sends the last known block hash of the canonical blockchain
            // and loops until the response is the same block (used to utilize
            // batch requests).
            let mut last = node.validator.read().await.blockchain.last()?;
            info!(target: "darkfid::task::sync_task", "Last known block: {:?} - {:?}", last.0, last.1);
            loop {
                // Node creates a `SyncRequest` and sends it
                let request = SyncRequest { slot: last.0, block: last.1 };
                channel.send(&request).await?;

                // Node stores response data
                let response = block_response_sub.receive().await?;

                // Verify and store retrieved blocks
                debug!(target: "darkfid::task::sync_task", "block_sync_task(): Processing received blocks");
                node.validator.write().await.add_blocks(&response.blocks).await?;

                let last_received = node.validator.read().await.blockchain.last()?;
                info!(target: "darkfid::task::sync_task", "Last received block: {:?} - {:?}", last_received.0, last_received.1);

                if last == last_received {
                    break
                }

                last = last_received;
            }
        }
        None => warn!(target: "darkfid::task::sync_task", "Node is not connected to other nodes"),
    };

    node.validator.write().await.synced = true;
    info!(target: "darkfid::task::sync_task", "Blockchain synced!");
    Ok(())
}
