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

use darkfi::{system::sleep, util::encoding::base64, Result};
use darkfi_serial::serialize_async;
use log::{debug, info, warn};
use tinyjson::JsonValue;

use crate::{
    proto::{
        ForkSyncRequest, ForkSyncResponse, IsSyncedRequest, IsSyncedResponse, SyncRequest,
        SyncResponse,
    },
    Darkfid,
};

/// async task used for block syncing
pub async fn sync_task(node: &Darkfid) -> Result<()> {
    info!(target: "darkfid::task::sync_task", "Starting blockchain sync...");
    // Block until at least node is connected to at least one synced peer
    let mut peers = vec![];
    loop {
        // Grab channels
        let channels = node.sync_p2p.channels().await;

        // Check anyone is connected
        if !channels.is_empty() {
            // Ask each peer if they are synced
            for channel in channels {
                // Communication setup
                let msg_subsystem = channel.message_subsystem();
                msg_subsystem.add_dispatch::<IsSyncedResponse>().await;
                let response_sub = channel.subscribe_msg::<IsSyncedResponse>().await?;

                // Node creates a `IsSyncedRequest` and sends it
                let request = IsSyncedRequest {};
                channel.send(&request).await?;

                // Node waits for response
                let Ok(response) = response_sub.receive_with_timeout(15).await else { continue };

                // Parse response
                if response.synced {
                    peers.push(channel)
                }
            }
        }

        // Check if we got peers to sync from
        if !peers.is_empty() {
            break
        }

        warn!(target: "darkfid::task::sync_task", "Node is not connected to other nodes, waiting to retry...");
        sleep(10).await;
    }

    // Getting a peer to ask for blocks
    let channel = &peers[0];

    // Communication setup
    let msg_subsystem = channel.message_subsystem();
    msg_subsystem.add_dispatch::<SyncResponse>().await;
    msg_subsystem.add_dispatch::<ForkSyncResponse>().await;
    let block_response_sub = channel.subscribe_msg::<SyncResponse>().await?;
    let proposals_response_sub = channel.subscribe_msg::<ForkSyncResponse>().await?;
    let notif_sub = node.subscribers.get("blocks").unwrap();
    let proposal_notif_sub = node.subscribers.get("proposals").unwrap();

    // TODO: make this parallel and use a head selection method,
    // for example use a manual known head and only connect to nodes
    // that follow that. Also use a random peer on every block range
    // we sync.

    // Node sends the last known block hash of the canonical blockchain
    // and loops until the response is the same block (used to utilize
    // batch requests).
    let mut last = node.validator.blockchain.last()?;
    info!(target: "darkfid::task::sync_task", "Last known block: {:?} - {:?}", last.0, last.1);
    loop {
        // Node creates a `SyncRequest` and sends it
        let request = SyncRequest { height: last.0 };
        channel.send(&request).await?;

        // TODO: add a timeout here to retry
        // Node waits for response
        let response = block_response_sub.receive().await?;

        // Verify and store retrieved blocks
        debug!(target: "darkfid::task::sync_task", "Processing received blocks");
        node.validator.add_blocks(&response.blocks).await?;

        // Notify subscriber
        for block in &response.blocks {
            let encoded_block = JsonValue::String(base64::encode(&serialize_async(block).await));
            notif_sub.notify(vec![encoded_block].into()).await;
        }

        let last_received = node.validator.blockchain.last()?;
        info!(target: "darkfid::task::sync_task", "Last received block: {:?} - {:?}", last_received.0, last_received.1);

        if last == last_received {
            break
        }

        last = last_received;
    }

    // Node syncs current best fork
    let request = ForkSyncRequest { tip: last.1, fork_tip: None };
    channel.send(&request).await?;

    // TODO: add a timeout here to retry
    // Node waits for response
    let response = proposals_response_sub.receive().await?;

    // Verify and store retrieved proposals
    debug!(target: "darkfid::task::sync_task", "Processing received proposals");
    for proposal in &response.proposals {
        node.validator.consensus.append_proposal(proposal).await?;
        // Notify subscriber
        let enc_prop = JsonValue::String(base64::encode(&serialize_async(proposal).await));
        proposal_notif_sub.notify(vec![enc_prop].into()).await;
    }

    *node.validator.synced.write().await = true;
    info!(target: "darkfid::task::sync_task", "Blockchain synced!");
    Ok(())
}
