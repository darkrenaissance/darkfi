/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::{collections::HashSet, sync::Arc};

use log::{debug, error, info};
use smol::{channel::Receiver, lock::RwLock};
use tinyjson::JsonValue;

use darkfi::{
    blockchain::BlockDifficulty,
    net::{ChannelPtr, P2pPtr},
    rpc::jsonrpc::JsonSubscriber,
    util::encoding::base64,
    validator::{
        consensus::{Fork, Proposal},
        pow::PoWModule,
        utils::{best_fork_index, header_rank},
        verification::verify_fork_proposal,
        ValidatorPtr,
    },
    Error, Result,
};
use darkfi_serial::serialize_async;

use crate::proto::{
    ForkHeaderHashRequest, ForkHeaderHashResponse, ForkHeadersRequest, ForkHeadersResponse,
    ForkProposalsRequest, ForkProposalsResponse, ForkSyncRequest, ForkSyncResponse,
    ProposalMessage, BATCH,
};

/// Background task to handle unknown proposals.
pub async fn handle_unknown_proposals(
    receiver: Receiver<(Proposal, u32)>,
    unknown_proposals: Arc<RwLock<HashSet<[u8; 32]>>>,
    validator: ValidatorPtr,
    p2p: P2pPtr,
    proposals_sub: JsonSubscriber,
    blocks_sub: JsonSubscriber,
) -> Result<()> {
    debug!(target: "darkfid::task::handle_unknown_proposal", "START");
    loop {
        // Wait for a new unknown proposal trigger
        let (proposal, channel) = match receiver.recv().await {
            Ok(m) => m,
            Err(e) => {
                debug!(
                    target: "darkfid::task::handle_unknown_proposal",
                    "recv fail: {e}"
                );
                continue
            }
        };

        // Check if proposal exists in our queue
        let lock = unknown_proposals.read().await;
        let contains_proposal = lock.contains(proposal.hash.inner());
        drop(lock);
        if !contains_proposal {
            debug!(
                target: "darkfid::task::handle_unknown_proposal",
                "Proposal {} is not in our unknown proposals queue.",
                proposal.hash,
            );
            continue
        };

        // Handle the unknown proposal
        if handle_unknown_proposal(
            &validator,
            &p2p,
            &proposals_sub,
            &blocks_sub,
            channel,
            &proposal,
        )
        .await
        {
            // Ban channel if it exists
            if let Some(channel) = p2p.get_channel(channel) {
                channel.ban().await;
            }
        };

        // Remove proposal from the queue
        let mut lock = unknown_proposals.write().await;
        lock.remove(proposal.hash.inner());
        drop(lock);
    }
}

/// Background task to handle an unknown proposal.
/// Returns a boolean flag indicate if we should ban the channel.
async fn handle_unknown_proposal(
    validator: &ValidatorPtr,
    p2p: &P2pPtr,
    proposals_sub: &JsonSubscriber,
    blocks_sub: &JsonSubscriber,
    channel: u32,
    proposal: &Proposal,
) -> bool {
    // If proposal fork chain was not found, we ask our peer for its sequence
    debug!(target: "darkfid::task::handle_unknown_proposal", "Asking peer for fork sequence");
    let Some(channel) = p2p.get_channel(channel) else {
        debug!(target: "darkfid::task::handle_unknown_proposal", "Channel {channel} wasn't found.");
        return false
    };

    // Communication setup
    let Ok(response_sub) = channel.subscribe_msg::<ForkSyncResponse>().await else {
        debug!(target: "darkfid::task::handle_unknown_proposal", "Failure during `ForkSyncResponse` communication setup with peer: {channel:?}");
        return true
    };

    // Grab last known block to create the request and execute it
    let last = match validator.blockchain.last() {
        Ok(l) => l,
        Err(e) => {
            error!(target: "darkfid::task::handle_unknown_proposal", "Blockchain last retriaval failed: {e}");
            return false
        }
    };
    let request = ForkSyncRequest { tip: last.1, fork_tip: Some(proposal.hash) };
    if let Err(e) = channel.send(&request).await {
        debug!(target: "darkfid::task::handle_unknown_proposal", "Channel send failed: {e}");
        return true
    };

    // Node waits for response
    let response = match response_sub
        .receive_with_timeout(p2p.settings().read().await.outbound_connect_timeout)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            debug!(target: "darkfid::task::handle_unknown_proposal", "Asking peer for fork sequence failed: {e}");
            return true
        }
    };
    debug!(target: "darkfid::task::handle_unknown_proposal", "Peer response: {response:?}");

    // Verify and store retrieved proposals
    debug!(target: "darkfid::task::handle_unknown_proposal", "Processing received proposals");

    // Response should not be empty
    if response.proposals.is_empty() {
        debug!(target: "darkfid::task::handle_unknown_proposal", "Peer responded with empty sequence, node might be out of sync!");
        return handle_reorg(validator, p2p, proposals_sub, blocks_sub, channel, proposal).await
    }

    // Sequence length must correspond to requested height
    if response.proposals.len() as u32 != proposal.block.header.height - last.0 {
        debug!(target: "darkfid::task::handle_unknown_proposal", "Response sequence length is erroneous");
        return handle_reorg(validator, p2p, proposals_sub, blocks_sub, channel, proposal).await
    }

    // First proposal must extend canonical
    if response.proposals[0].block.header.previous != last.1 {
        debug!(target: "darkfid::task::handle_unknown_proposal", "Response sequence doesn't extend canonical");
        return handle_reorg(validator, p2p, proposals_sub, blocks_sub, channel, proposal).await
    }

    // Last proposal must be the same as the one requested
    if response.proposals.last().unwrap().hash != proposal.hash {
        debug!(target: "darkfid::task::handle_unknown_proposal", "Response sequence doesn't correspond to requested tip");
        return handle_reorg(validator, p2p, proposals_sub, blocks_sub, channel, proposal).await
    }

    // Process response proposals
    for proposal in &response.proposals {
        // Append proposal
        match validator.append_proposal(proposal).await {
            Ok(()) => { /* Do nothing */ }
            // Skip already existing proposals
            Err(Error::ProposalAlreadyExists) => continue,
            Err(e) => {
                debug!(
                    target: "darkfid::task::handle_unknown_proposal",
                    "Error while appending response proposal: {e}"
                );
                break;
            }
        };

        // Broadcast proposal to rest nodes
        let message = ProposalMessage(proposal.clone());
        p2p.broadcast_with_exclude(&message, &[channel.address().clone()]).await;

        // Notify proposals subscriber
        let enc_prop = JsonValue::String(base64::encode(&serialize_async(proposal).await));
        proposals_sub.notify(vec![enc_prop].into()).await;
    }

    false
}

/// Auxiliary function to handle a potential reorg.
/// We first find our last common block with the peer,
/// then grab the header sequence from that block until
/// the proposal and check if it ranks higher than our
/// current best ranking fork, to perform a reorg.
/// Returns a boolean flag indicate if we should ban the
/// channel.
async fn handle_reorg(
    validator: &ValidatorPtr,
    p2p: &P2pPtr,
    proposals_sub: &JsonSubscriber,
    blocks_sub: &JsonSubscriber,
    channel: ChannelPtr,
    proposal: &Proposal,
) -> bool {
    info!(target: "darkfid::task::handle_reorg", "Checking for potential reorg from proposal {} - {} by peer: {channel:?}", proposal.hash, proposal.block.header.height);

    // Check if genesis proposal was provided
    if proposal.block.header.height == 0 {
        debug!(target: "darkfid::task::handle_reorg", "Peer send a genesis proposal, skipping...");
        return true
    }

    // Communication setup
    let Ok(response_sub) = channel.subscribe_msg::<ForkHeaderHashResponse>().await else {
        debug!(target: "darkfid::task::handle_reorg", "Failure during `ForkHeaderHashResponse` communication setup with peer: {channel:?}");
        return true
    };

    // Keep track of received header hashes sequence
    let mut peer_header_hashes = vec![];

    // Find last common header, going backwards from the proposal
    let mut previous_height = proposal.block.header.height;
    let mut previous_hash = proposal.hash;
    for height in (0..proposal.block.header.height).rev() {
        // Request peer header hash for this height
        let request = ForkHeaderHashRequest { height, fork_header: proposal.hash };
        if let Err(e) = channel.send(&request).await {
            debug!(target: "darkfid::task::handle_reorg", "Channel send failed: {e}");
            return true
        };

        // Node waits for response
        let response = match response_sub
            .receive_with_timeout(p2p.settings().read().await.outbound_connect_timeout)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                debug!(target: "darkfid::task::handle_reorg", "Asking peer for header hash failed: {e}");
                return true
            }
        };
        debug!(target: "darkfid::task::handle_reorg", "Peer response: {response:?}");

        // Check if peer returned a header
        let Some(peer_header) = response.fork_header else {
            debug!(target: "darkfid::task::handle_reorg", "Peer responded with an empty header");
            return true
        };

        // Check if we know this header
        let headers = match validator.blockchain.blocks.get_order(&[height], false) {
            Ok(r) => r,
            Err(e) => {
                error!(target: "darkfid::task::handle_reorg", "Retrieving headers failed: {e}");
                return false
            }
        };
        match headers[0] {
            Some(known_header) => {
                if known_header == peer_header {
                    previous_height = height;
                    previous_hash = known_header;
                    break
                }
                // Since we retrieve in right -> left order we push them in reverse order
                peer_header_hashes.insert(0, peer_header);
            }
            None => peer_header_hashes.insert(0, peer_header),
        }
    }

    // Check if we have a sequence to process
    if peer_header_hashes.is_empty() {
        debug!(target: "darkfid::task::handle_reorg", "No headers to process, skipping...");
        return true
    }

    // Communication setup
    let Ok(response_sub) = channel.subscribe_msg::<ForkHeadersResponse>().await else {
        debug!(target: "darkfid::task::handle_reorg", "Failure during `ForkHeadersResponse` communication setup with peer: {channel:?}");
        return true
    };

    // Grab last common height ranks
    let last_common_height = previous_height;
    let last_difficulty = match previous_height {
        0 => {
            let genesis_timestamp = match validator.blockchain.genesis_block() {
                Ok(b) => b.header.timestamp,
                Err(e) => {
                    error!(target: "darkfid::task::handle_reorg", "Retrieving genesis block failed: {e}");
                    return false
                }
            };
            BlockDifficulty::genesis(genesis_timestamp)
        }
        _ => match validator.blockchain.blocks.get_difficulty(&[last_common_height], true) {
            Ok(d) => d[0].clone().unwrap(),
            Err(e) => {
                error!(target: "darkfid::task::handle_reorg", "Retrieving block difficulty failed: {e}");
                return false
            }
        },
    };

    // Create a new PoW from last common height
    let module = match PoWModule::new(
        validator.consensus.blockchain.clone(),
        validator.consensus.module.read().await.target,
        validator.consensus.module.read().await.fixed_difficulty.clone(),
        Some(last_common_height + 1),
    ) {
        Ok(m) => m,
        Err(e) => {
            error!(target: "darkfid::task::handle_reorg", "PoWModule generation failed: {e}");
            return false
        }
    };

    // Retrieve the headers of the hashes sequence, in batches, keeping track of the sequence ranking
    info!(target: "darkfid::task::handle_reorg", "Retrieving {} headers from peer...", peer_header_hashes.len());
    let mut batch = Vec::with_capacity(BATCH);
    let mut total_processed = 0;
    let mut targets_rank = last_difficulty.ranks.targets_rank.clone();
    let mut hashes_rank = last_difficulty.ranks.hashes_rank.clone();
    let mut headers_module = module.clone();
    for (index, hash) in peer_header_hashes.iter().enumerate() {
        // Add hash in batch sequence
        batch.push(*hash);

        // Check if batch is full so we can send it
        if batch.len() < BATCH && index != peer_header_hashes.len() - 1 {
            continue
        }

        // Request peer headers
        let request = ForkHeadersRequest { headers: batch.clone(), fork_header: proposal.hash };
        if let Err(e) = channel.send(&request).await {
            debug!(target: "darkfid::task::handle_reorg", "Channel send failed: {e}");
            return true
        };

        // Node waits for response
        let response = match response_sub
            .receive_with_timeout(p2p.settings().read().await.outbound_connect_timeout)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                debug!(target: "darkfid::task::handle_reorg", "Asking peer for headers sequence failed: {e}");
                return true
            }
        };
        debug!(target: "darkfid::task::handle_reorg", "Peer response: {response:?}");

        // Response sequence must be the same length as the one requested
        if response.headers.len() != batch.len() {
            debug!(target: "darkfid::task::handle_reorg", "Peer responded with a different headers sequence length");
            return true
        }

        // Process retrieved headers
        for (peer_header_index, peer_header) in response.headers.iter().enumerate() {
            let peer_header_hash = peer_header.hash();
            debug!(target: "darkfid::task::handle_reorg", "Processing header: {peer_header_hash} - {}", peer_header.height);

            // Validate its the header we requested
            if peer_header_hash != batch[peer_header_index] {
                debug!(target: "darkfid::task::handle_reorg", "Peer responded with a differend header: {} - {peer_header_hash}", batch[peer_header_index]);
                return true
            }

            // Validate sequence is correct
            if peer_header.previous != previous_hash || peer_header.height != previous_height + 1 {
                debug!(target: "darkfid::task::handle_reorg", "Invalid header sequence detected");
                return true
            }

            // Grab next mine target and difficulty
            let (next_target, next_difficulty) = match headers_module
                .next_mine_target_and_difficulty()
            {
                Ok(p) => p,
                Err(e) => {
                    debug!(target: "darkfid::task::handle_reorg", "Retrieving next mine target and difficulty failed: {e}");
                    return false
                }
            };

            // Verify header hash and calculate its rank
            let (target_distance_sq, hash_distance_sq) = match header_rank(
                peer_header,
                &next_target,
            ) {
                Ok(distances) => distances,
                Err(e) => {
                    debug!(target: "darkfid::task::handle_reorg", "Invalid header hash detected: {e}");
                    return true
                }
            };

            // Update sequence ranking
            targets_rank += target_distance_sq.clone();
            hashes_rank += hash_distance_sq.clone();

            // Update PoW headers module
            headers_module.append(peer_header.timestamp, &next_difficulty);

            // Set previous header
            previous_height = peer_header.height;
            previous_hash = peer_header_hash;
        }

        total_processed += response.headers.len();
        info!(target: "darkfid::task::handle_reorg", "Headers received and verified: {total_processed}/{}", peer_header_hashes.len());

        // Reset batch
        batch = Vec::with_capacity(BATCH);
    }

    // Check if the sequence ranks higher than our current best fork
    let forks = validator.consensus.forks.read().await;
    let index = match best_fork_index(&forks) {
        Ok(i) => i,
        Err(e) => {
            debug!(target: "darkfid::task::handle_reorg", "Retrieving best fork index failed: {e}");
            return false
        }
    };
    let best_fork = &forks[index];
    if targets_rank < best_fork.targets_rank ||
        (targets_rank == best_fork.targets_rank && hashes_rank <= best_fork.hashes_rank)
    {
        info!(target: "darkfid::task::handle_reorg", "Peer sequence ranks lower than our current best fork, skipping...");
        drop(forks);
        return true
    }
    drop(forks);

    // Communication setup
    let Ok(response_sub) = channel.subscribe_msg::<ForkProposalsResponse>().await else {
        debug!(target: "darkfid::task::handle_reorg", "Failure during `ForkProposalsResponse` communication setup with peer: {channel:?}");
        return true
    };

    // Create a fork from last common height
    let mut peer_fork =
        match Fork::new(validator.consensus.blockchain.clone(), module.clone()).await {
            Ok(f) => f,
            Err(e) => {
                error!(target: "darkfid::task::handle_reorg", "Generating peer fork failed: {e}");
                return false
            }
        };
    peer_fork.targets_rank = last_difficulty.ranks.targets_rank.clone();
    peer_fork.hashes_rank = last_difficulty.ranks.hashes_rank.clone();

    // Grab all state inverse diffs after last common height, and add them to the fork
    let inverse_diffs = match validator
        .blockchain
        .blocks
        .get_state_inverse_diffs_after(last_common_height)
    {
        Ok(i) => i,
        Err(e) => {
            error!(target: "darkfid::task::handle_reorg", "Retrieving state inverse diffs failed: {e}");
            return false
        }
    };
    for inverse_diff in inverse_diffs.iter().rev() {
        if let Err(e) =
            peer_fork.overlay.lock().unwrap().overlay.lock().unwrap().add_diff(inverse_diff)
        {
            error!(target: "darkfid::task::handle_reorg", "Applying inverse diff failed: {e}");
            return false
        }
    }

    // Rebuild fork contracts states monotree
    if let Err(e) = peer_fork.compute_monotree() {
        error!(target: "darkfid::task::handle_reorg", "Rebuilding peer fork monotree failed: {e}");
        return false
    }

    // Retrieve the proposals of the hashes sequence, in batches
    info!(target: "darkfid::task::handle_reorg", "Peer sequence ranks higher than our current best fork, retrieving {} proposals from peer...", peer_header_hashes.len());
    let mut batch = Vec::with_capacity(BATCH);
    let mut total_processed = 0;
    for (index, hash) in peer_header_hashes.iter().enumerate() {
        // Add hash in batch sequence
        batch.push(*hash);

        // Check if batch is full so we can send it
        if batch.len() < BATCH && index != peer_header_hashes.len() - 1 {
            continue
        }

        // Request peer proposals
        let request = ForkProposalsRequest { headers: batch.clone(), fork_header: proposal.hash };
        if let Err(e) = channel.send(&request).await {
            debug!(target: "darkfid::task::handle_reorg", "Channel send failed: {e}");
            return true
        };

        // Node waits for response
        let response = match response_sub
            .receive_with_timeout(p2p.settings().read().await.outbound_connect_timeout)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                debug!(target: "darkfid::task::handle_reorg", "Asking peer for proposals sequence failed: {e}");
                return true
            }
        };
        debug!(target: "darkfid::task::handle_reorg", "Peer response: {response:?}");

        // Response sequence must be the same length as the one requested
        if response.proposals.len() != batch.len() {
            debug!(target: "darkfid::task::handle_reorg", "Peer responded with a different proposals sequence length");
            return true
        }

        // Process retrieved proposal
        for (peer_proposal_index, peer_proposal) in response.proposals.iter().enumerate() {
            info!(target: "darkfid::task::handle_reorg", "Processing proposal: {} - {}", peer_proposal.hash, peer_proposal.block.header.height);

            // Validate its the proposal we requested
            if peer_proposal.hash != batch[peer_proposal_index] {
                error!(target: "darkfid::task::handle_reorg", "Peer responded with a differend proposal: {} - {}", batch[peer_proposal_index], peer_proposal.hash);
                return true
            }

            // Verify proposal
            if let Err(e) =
                verify_fork_proposal(&mut peer_fork, peer_proposal, validator.verify_fees).await
            {
                error!(target: "darkfid::task::handle_reorg", "Verify fork proposal failed: {e}");
                return true
            }

            // Append proposal
            if let Err(e) = peer_fork.append_proposal(peer_proposal).await {
                error!(target: "darkfid::task::handle_reorg", "Appending proposal failed: {e}");
                return true
            }
        }

        total_processed += response.proposals.len();
        info!(target: "darkfid::task::handle_reorg", "Proposals received and verified: {total_processed}/{}", peer_header_hashes.len());

        // Reset batch
        batch = Vec::with_capacity(BATCH);
    }

    // Verify trigger proposal
    if let Err(e) = verify_fork_proposal(&mut peer_fork, proposal, validator.verify_fees).await {
        error!(target: "darkfid::task::handle_reorg", "Verify proposal failed: {e}");
        return true
    }

    // Append trigger proposal
    if let Err(e) = peer_fork.append_proposal(proposal).await {
        error!(target: "darkfid::task::handle_reorg", "Appending proposal failed: {e}");
        return true
    }

    // Check if the peer fork ranks higher than our current best fork
    let mut forks = validator.consensus.forks.write().await;
    let index = match best_fork_index(&forks) {
        Ok(i) => i,
        Err(e) => {
            debug!(target: "darkfid::task::handle_reorg", "Retrieving best fork index failed: {e}");
            return false
        }
    };
    let best_fork = &forks[index];
    if peer_fork.targets_rank < best_fork.targets_rank ||
        (peer_fork.targets_rank == best_fork.targets_rank &&
            peer_fork.hashes_rank <= best_fork.hashes_rank)
    {
        info!(target: "darkfid::task::handle_reorg", "Peer fork ranks lower than our current best fork, skipping...");
        drop(forks);
        return true
    }

    // Execute the reorg
    info!(target: "darkfid::task::handle_reorg", "Peer fork ranks higher than our current best fork, executing reorg...");
    *validator.consensus.module.write().await = module;
    *forks = vec![peer_fork];
    drop(forks);

    // Check if we can confirm anything and broadcast them
    let confirmed = match validator.confirmation().await {
        Ok(f) => f,
        Err(e) => {
            error!(target: "darkfid::task::handle_reorg", "Confirmation failed: {e}");
            return false
        }
    };

    if !confirmed.is_empty() {
        let mut notif_blocks = Vec::with_capacity(confirmed.len());
        for block in confirmed {
            notif_blocks.push(JsonValue::String(base64::encode(&serialize_async(&block).await)));
        }
        blocks_sub.notify(JsonValue::Array(notif_blocks)).await;
    }

    // Broadcast proposal to the network
    let message = ProposalMessage(proposal.clone());
    p2p.broadcast(&message).await;

    // Notify proposals subscriber
    let enc_prop = JsonValue::String(base64::encode(&serialize_async(proposal).await));
    proposals_sub.notify(vec![enc_prop].into()).await;

    false
}
