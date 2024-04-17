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

use std::collections::HashMap;

use darkfi::{
    blockchain::HeaderHash, net::ChannelPtr, system::sleep, util::encoding::base64, Error, Result,
};
use darkfi_serial::serialize_async;
use log::{debug, error, info, warn};
use rand::{prelude::SliceRandom, rngs::OsRng};
use tinyjson::JsonValue;

use crate::{
    proto::{
        ForkSyncRequest, ForkSyncResponse, HeaderSyncRequest, HeaderSyncResponse, IsSyncedRequest,
        IsSyncedResponse, SyncRequest, SyncResponse, TipRequest, TipResponse, BATCH, COMMS_TIMEOUT,
    },
    Darkfid,
};

// TODO: Parallelize independent requests.
//       We can also make them be like torrents, where we retrieve chunks not in order.
/// async task used for block syncing.
pub async fn sync_task(node: &Darkfid) -> Result<()> {
    info!(target: "darkfid::task::sync_task", "Starting blockchain sync...");

    // Grab synced peers
    let peers = synced_peers(node).await?;

    // TODO: Configure a checkpoint, filter peers that don't have that and start
    // syncing the sequence until that

    // Grab last known block header
    let mut last = last_header(node)?;
    info!(target: "darkfid::task::sync_task", "Last known block: {} - {}", last.0, last.1);
    loop {
        // Grab the most common tip and the corresponding peers
        let (common_tip_height, common_tip_peers) = most_common_tip(&peers, &last.1).await?;

        // Retrieve all the headers backawards until our last known one and verify them.
        // We use the next height, in order to also retrieve the peers tip header.
        retrieve_headers(node, &common_tip_peers, last, common_tip_height + 1).await?;

        // Retrieve all the blocks for those headers and apply them to canonical
        retrieve_blocks(node, &peers).await?;

        let last_received = last_header(node)?;
        info!(target: "darkfid::task::sync_task", "Last received block: {} - {}", last_received.0, last_received.1);

        if last == last_received {
            break
        }

        last = last_received;
    }

    // Sync best fork
    sync_best_fork(node, &peers, &last.1).await?;

    *node.validator.synced.write().await = true;
    info!(target: "darkfid::task::sync_task", "Blockchain synced!");
    Ok(())
}

/// Auxiliary function to lock until node is connected to at least one synced peer.
async fn synced_peers(node: &Darkfid) -> Result<Vec<ChannelPtr>> {
    let mut peers = vec![];
    loop {
        // Grab channels
        let channels = node.p2p.hosts().channels().await;

        // Check anyone is connected
        if !channels.is_empty() {
            // Ask each peer if they are synced
            for channel in channels {
                // Communication setup
                let response_sub = channel.subscribe_msg::<IsSyncedResponse>().await?;

                // Node creates a `IsSyncedRequest` and sends it
                let request = IsSyncedRequest {};
                channel.send(&request).await?;

                // Node waits for response
                let Ok(response) = response_sub.receive_with_timeout(COMMS_TIMEOUT).await else {
                    continue
                };

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

    Ok(peers)
}

/// Auxiliary function to retrieve last known block header, including existing pending sync ones.
fn last_header(node: &Darkfid) -> Result<(u32, HeaderHash)> {
    // First we check if we have pending sync headers
    if let Some(last_sync) = node.validator.blockchain.headers.get_last_sync()? {
        return Ok((last_sync.height, last_sync.hash()))
    }
    // Then we grab the last one from the actual canonical chain
    node.validator.blockchain.last()
}

/// Auxiliary function to ask all peers for their current tip and find the most common one.
async fn most_common_tip(
    peers: &[ChannelPtr],
    last_tip: &HeaderHash,
) -> Result<(u32, Vec<ChannelPtr>)> {
    info!(target: "darkfid::task::sync::most_common_tip", "Receiving tip from peers...");
    let mut tips: HashMap<(u32, [u8; 32]), Vec<ChannelPtr>> = HashMap::new();
    for peer in peers {
        // Node creates a `TipRequest` and sends it
        let response_sub = peer.subscribe_msg::<TipResponse>().await?;
        let request = TipRequest { tip: *last_tip };
        peer.send(&request).await?;

        // Node waits for response
        let Ok(response) = response_sub.receive_with_timeout(COMMS_TIMEOUT).await else { continue };

        // Handle response
        let tip = (response.height, *response.hash.inner());
        let Some(tip_peers) = tips.get_mut(&tip) else {
            tips.insert(tip, vec![peer.clone()]);
            continue
        };
        tip_peers.push(peer.clone());
    }

    // Grab the most common tip peers
    let mut common_tips = vec![];
    let mut common_tip_peers = vec![];
    for (tip, peers) in tips {
        if peers.len() < common_tip_peers.len() {
            continue;
        }
        if peers.len() == common_tip_peers.len() {
            common_tips.push(tip);
            continue;
        }
        common_tips = vec![tip];
        common_tip_peers = peers;
    }
    if common_tips.len() > 1 {
        error!(target: "darkfid::task::sync::most_common_tip", "Multiple common tips found: {:?}", common_tips);
        return Err(Error::BlockchainSyncError)
    }

    info!(target: "darkfid::task::sync::most_common_tip", "Received tip from peers: {} - {}", common_tips[0].0, HeaderHash::new(common_tips[0].1));
    Ok((common_tips[0].0, common_tip_peers))
}

/// Auxiliary function to retrieve headers backwards until our last known one and verify them.
async fn retrieve_headers(
    node: &Darkfid,
    peers: &[ChannelPtr],
    last_known: (u32, HeaderHash),
    tip_height: u32,
) -> Result<()> {
    info!(target: "darkfid::task::sync::retrieve_headers", "Retrieving missing headers from peers...");
    // Communication setup
    let mut peer_subs = vec![];
    for peer in peers {
        peer_subs.push(peer.subscribe_msg::<HeaderSyncResponse>().await?);
    }

    // We subtract 1 since tip_height is increased by one
    let total = tip_height - last_known.0 - 1;
    let mut last_tip_height = tip_height;
    'headers_loop: loop {
        for (index, peer) in peers.iter().enumerate() {
            // Node creates a `HeaderSyncRequest` and sends it
            let request = HeaderSyncRequest { height: last_tip_height };
            peer.send(&request).await?;

            // Node waits for response
            let Ok(response) = peer_subs[index].receive_with_timeout(COMMS_TIMEOUT).await else {
                continue
            };

            // Retain only the headers after our last known
            let mut response_headers = response.headers.to_vec();
            response_headers.retain(|h| h.height > last_known.0);

            if response_headers.is_empty() {
                break 'headers_loop
            }

            // Store the headers
            node.validator.blockchain.headers.insert_sync(&response_headers)?;
            last_tip_height = response_headers[0].height;
            info!(target: "darkfid::task::sync::retrieve_headers", "Headers received: {}/{}", node.validator.blockchain.headers.len_sync(), total);
        }
    }

    // Check if we retrieved any new headers
    if node.validator.blockchain.headers.is_empty_sync() {
        return Ok(());
    }

    // Verify headers sequence. Here we do a quick and dirty verification
    // of just the hashes and heights sequence. We will formaly verify
    // the blocks when we retrieve them. We verify them in batches,
    // to not load them all in memory.
    info!(target: "darkfid::task::sync::retrieve_headers", "Verifying headers sequence...");
    let mut verified_headers = 0;
    let total = node.validator.blockchain.headers.len_sync();
    // First we verify the first `BATCH` sequence, using the last canonical known one as
    // the first sync header previous.
    let last_known = node.validator.blockchain.last()?;
    let mut headers = node.validator.blockchain.headers.get_after_sync(0, BATCH)?;
    if headers[0].previous != last_known.1 || headers[0].height != last_known.0 + 1 {
        return Err(Error::BlockIsInvalid(headers[0].hash().as_string()))
    }
    verified_headers += 1;
    for (index, header) in headers[1..].iter().enumerate() {
        if header.previous != headers[index].hash() || header.height != headers[index].height + 1 {
            return Err(Error::BlockIsInvalid(header.hash().as_string()))
        }
        verified_headers += 1;
    }
    info!(target: "darkfid::task::sync::retrieve_headers", "Headers verified: {}/{}", verified_headers, total);
    // Now we verify the rest sequences
    let mut last_checked = headers.last().unwrap().clone();
    headers = node.validator.blockchain.headers.get_after_sync(last_checked.height, BATCH)?;
    while !headers.is_empty() {
        if headers[0].previous != last_checked.hash() ||
            headers[0].height != last_checked.height + 1
        {
            return Err(Error::BlockIsInvalid(headers[0].hash().as_string()))
        }
        verified_headers += 1;
        for (index, header) in headers[1..].iter().enumerate() {
            if header.previous != headers[index].hash() ||
                header.height != headers[index].height + 1
            {
                return Err(Error::BlockIsInvalid(header.hash().as_string()))
            }
            verified_headers += 1;
        }
        last_checked = headers.last().unwrap().clone();
        headers = node.validator.blockchain.headers.get_after_sync(last_checked.height, BATCH)?;
        info!(target: "darkfid::task::sync::retrieve_headers", "Headers verified: {}/{}", verified_headers, total);
    }

    info!(target: "darkfid::task::sync::retrieve_headers", "Headers sequence verified!");
    Ok(())
}

/// Auxiliary function to retrieve blocks of provided headers and apply them to canonical.
async fn retrieve_blocks(node: &Darkfid, peers: &[ChannelPtr]) -> Result<()> {
    info!(target: "darkfid::task::sync::retrieve_blocks", "Retrieving missing blocks from peers...");
    // Communication setup
    let mut peer_subs = vec![];
    for peer in peers {
        peer_subs.push(peer.subscribe_msg::<SyncResponse>().await?);
    }
    let notif_sub = node.subscribers.get("blocks").unwrap();

    let mut received_blocks = 0;
    let total = node.validator.blockchain.headers.len_sync();
    'blocks_loop: loop {
        for (index, peer) in peers.iter().enumerate() {
            // Grab first `BATCH` headers
            let headers = node.validator.blockchain.headers.get_after_sync(0, BATCH)?;
            if headers.is_empty() {
                break 'blocks_loop
            }

            // Node creates a `SyncRequest` and sends it
            let request = SyncRequest { headers: headers.iter().map(|h| h.hash()).collect() };
            peer.send(&request).await?;

            // Node waits for response
            let Ok(response) = peer_subs[index].receive_with_timeout(COMMS_TIMEOUT).await else {
                continue
            };

            // Verify and store retrieved blocks
            debug!(target: "darkfid::task::sync::retrieve_blocks", "Processing received blocks");
            node.validator.add_blocks(&response.blocks).await?;

            // Remove synced headers
            node.validator.blockchain.headers.remove_sync(
                &response.blocks.iter().map(|b| b.header.height).collect::<Vec<u32>>(),
            )?;

            // Notify subscriber
            for block in &response.blocks {
                info!(target: "darkfid::task::sync::retrieve_blocks", "Appended block: {} - {}", block.header.height, block.hash());
                let encoded_block =
                    JsonValue::String(base64::encode(&serialize_async(block).await));
                notif_sub.notify(vec![encoded_block].into()).await;
            }

            received_blocks += response.blocks.len();
            info!(target: "darkfid::task::sync::retrieve_blocks", "Blocks received: {}/{}", received_blocks, total);
        }
    }

    Ok(())
}

/// Auxiliary function to retrieve best fork state from a random peer.
async fn sync_best_fork(node: &Darkfid, peers: &[ChannelPtr], last_tip: &HeaderHash) -> Result<()> {
    info!(target: "darkfid::task::sync::sync_best_fork", "Syncing fork states from peers...");
    // Getting a random peer to ask for blocks
    let channel = &peers.choose(&mut OsRng).unwrap();

    // Communication setup
    let response_sub = channel.subscribe_msg::<ForkSyncResponse>().await?;
    let notif_sub = node.subscribers.get("proposals").unwrap();

    // Node creates a `ForkSyncRequest` and sends it
    let request = ForkSyncRequest { tip: *last_tip, fork_tip: None };
    channel.send(&request).await?;

    // Node waits for response
    let response = response_sub.receive_with_timeout(COMMS_TIMEOUT).await?;

    // Verify and store retrieved proposals
    debug!(target: "darkfid::task::sync_task", "Processing received proposals");
    for proposal in &response.proposals {
        node.validator.append_proposal(proposal).await?;
        // Notify subscriber
        let enc_prop = JsonValue::String(base64::encode(&serialize_async(proposal).await));
        notif_sub.notify(vec![enc_prop].into()).await;
    }

    Ok(())
}
