/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use num_bigint::BigUint;
use smol::{channel::Receiver, lock::RwLock};
use tinyjson::JsonValue;
use tracing::{debug, error, info};

use darkfi::{
    blockchain::{BlockDifficulty, HeaderHash},
    net::{ChannelPtr, P2pPtr},
    rpc::jsonrpc::JsonSubscriber,
    util::{encoding::base64, time::Timestamp},
    validator::{
        consensus::{Fork, Proposal},
        pow::PoWModule,
        utils::{best_fork_index, header_rank},
        verification::verify_fork_proposal,
        ValidatorPtr,
    },
    Error::{Custom, DatabaseError, PoWInvalidOutHash, ProposalAlreadyExists},
    Result,
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
    unknown_proposals_channels: Arc<RwLock<HashMap<u32, (u8, u64)>>>,
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

        // Increase channel counter
        let mut lock = unknown_proposals_channels.write().await;
        let channel_counter = if let Some((counter, timestamp)) = lock.get_mut(&channel) {
            *counter += 1;
            *timestamp = Timestamp::current_time().inner();
            *counter
        } else {
            lock.insert(channel, (1, Timestamp::current_time().inner()));
            1
        };
        drop(lock);

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
            // Ban channel if it exceeds 5 consecutive unknown proposals
            if channel_counter > 5 {
                if let Some(channel) = p2p.get_channel(channel) {
                    channel.ban().await;
                }
                unknown_proposals_channels.write().await.remove(&channel);
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

    let comms_timeout =
        p2p.settings().read_arc().await.outbound_connect_timeout(channel.address().scheme());

    // Node waits for response
    let response = match response_sub.receive_with_timeout(comms_timeout).await {
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
            Err(ProposalAlreadyExists) => continue,
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

/// Auxiliary function to handle a potential reorg. We first find our
/// last common block with the peer, then grab the header sequence from
/// that block until the proposal and check if it ranks higher than our
/// current best ranking fork, to perform a reorg.
///
/// Returns a boolean flag indicate if we should ban the channel.
///
/// Note: Always remember to purge new trees from the database if not
/// needed.
//  TODO: We keep everything in memory which can result in OOM for a
//        valid long fork. We could use some disk space to store stuff.
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

    // Retrieve communications timeout
    let comms_timeout =
        p2p.settings().read_arc().await.outbound_connect_timeout(channel.address().scheme());

    // Find last common header and its sequence, going backwards from
    // the proposal.
    let (last_common_height, last_common_hash, peer_header_hashes) =
        match retrieve_peer_header_hashes(validator, (&channel, &comms_timeout), proposal).await {
            Ok(t) => t,
            Err(DatabaseError(e)) => {
                error!(target: "darkfid::task::handle_reorg", "Internal error while retrieving peer headers hashes: {e}");
                return false
            }
            Err(e) => {
                error!(target: "darkfid::task::handle_reorg", "Retrieving peer headers hashes failed: {e}");
                return true
            }
        };

    // Create a new PoW module from last common height
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

    // Grab last common height ranks
    let last_difficulty = match last_common_height {
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

    // Retrieve the headers of the hashes sequence and its ranking
    let (targets_rank, hashes_rank) = match retrieve_peer_headers_sequence_ranking(
        (&last_common_height, &last_common_hash, &module, &last_difficulty),
        (&channel, &comms_timeout),
        proposal,
        &peer_header_hashes,
    )
    .await
    {
        Ok(p) => p,
        Err(DatabaseError(e)) => {
            error!(target: "darkfid::task::handle_reorg", "Internal error while retrieving peer headers: {e}");
            return false
        }
        Err(e) => {
            error!(target: "darkfid::task::handle_reorg", "Retrieving peer headers failed: {e}");
            return true
        }
    };

    // Grab the append lock so no other proposal gets processed while
    // we are verifying the sequence.
    let append_lock = validator.consensus.append_lock.write().await;

    // Check if the sequence ranks higher than our current best fork
    let mut forks = validator.consensus.forks.write().await;
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
        return true
    }

    // Generate the peer fork and retrieve its ranking
    let peer_fork = match retrieve_peer_fork(
        validator,
        (&last_common_height, &module, &last_difficulty),
        (&channel, &comms_timeout),
        proposal,
        &peer_header_hashes,
    )
    .await
    {
        Ok(p) => p,
        Err(DatabaseError(e)) => {
            error!(target: "darkfid::task::handle_reorg", "Internal error while retrieving peer fork: {e}");
            return false
        }
        Err(e) => {
            error!(target: "darkfid::task::handle_reorg", "Retrieving peer fork failed: {e}");
            return true
        }
    };

    // Check if the peer fork ranks higher than our current best fork
    if peer_fork.targets_rank < best_fork.targets_rank ||
        (peer_fork.targets_rank == best_fork.targets_rank &&
            peer_fork.hashes_rank <= best_fork.hashes_rank)
    {
        info!(target: "darkfid::task::handle_reorg", "Peer fork ranks lower than our current best fork, skipping...");
        return true
    }

    // Execute the reorg
    info!(target: "darkfid::task::handle_reorg", "Peer fork ranks higher than our current best fork, executing reorg...");
    if let Err(e) = validator.blockchain.reset_to_height(last_common_height) {
        error!(target: "darkfid::task::handle_reorg", "Applying full inverse diff failed: {e}");
        return false
    };
    *validator.consensus.module.write().await = module;
    *forks = vec![peer_fork];
    drop(forks);
    drop(append_lock);

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

/// Auxiliary function to retrieve the last common header and height,
/// along with the headers sequence up to provided peer proposal.
async fn retrieve_peer_header_hashes(
    // Validator pointer
    validator: &ValidatorPtr,
    // Peer channel and its communications timeout
    channel: (&ChannelPtr, &u64),
    // Peer fork proposal
    proposal: &Proposal,
) -> Result<(u32, HeaderHash, Vec<HeaderHash>)> {
    // Communication setup
    let response_sub = channel.0.subscribe_msg::<ForkHeaderHashResponse>().await?;

    // Keep track of received header hashes sequence
    let mut peer_header_hashes = vec![];

    // Find last common header, going backwards from the proposal
    let mut previous_height = proposal.block.header.height;
    let mut previous_hash = proposal.hash;
    for height in (0..proposal.block.header.height).rev() {
        // Request peer header hash for this height
        let request = ForkHeaderHashRequest { height, fork_header: proposal.hash };
        channel.0.send(&request).await?;

        // Node waits for response
        let response = response_sub.receive_with_timeout(*channel.1).await?;
        debug!(target: "darkfid::task::handle_reorg", "Peer response: {response:?}");

        // Check if peer returned a header
        let Some(peer_header) = response.fork_header else {
            return Err(Custom(String::from("Peer responded with an empty header")))
        };

        // Check if we know this header
        let headers = match validator.blockchain.blocks.get_order(&[height], false) {
            Ok(h) => h,
            Err(e) => return Err(DatabaseError(format!("Retrieving headers failed: {e}"))),
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

    Ok((previous_height, previous_hash, peer_header_hashes))
}

/// Auxiliary function to retrieve provided peer headers hashes
/// sequence and its ranking, based on provided last common
/// information.
async fn retrieve_peer_headers_sequence_ranking(
    // Last common header, PoW module and difficulty
    last_common_info: (&u32, &HeaderHash, &PoWModule, &BlockDifficulty),
    // Peer channel and its communications timeout
    channel: (&ChannelPtr, &u64),
    // Peer fork trigger proposal
    proposal: &Proposal,
    // Peer header hashes sequence
    header_hashes: &[HeaderHash],
) -> Result<(BigUint, BigUint)> {
    // Communication setup
    let response_sub = channel.0.subscribe_msg::<ForkHeadersResponse>().await?;

    // Retrieve the headers of the hashes sequence, in batches, keeping track of the sequence ranking
    info!(target: "darkfid::task::handle_reorg", "Retrieving {} headers from peer...", header_hashes.len());
    let mut previous_height = *last_common_info.0;
    let mut previous_hash = *last_common_info.1;
    let mut module = last_common_info.2.clone();
    let mut targets_rank = last_common_info.3.ranks.targets_rank.clone();
    let mut hashes_rank = last_common_info.3.ranks.hashes_rank.clone();
    let mut batch = Vec::with_capacity(BATCH);
    let mut total_processed = 0;
    for (index, hash) in header_hashes.iter().enumerate() {
        // Add hash in batch sequence
        batch.push(*hash);

        // Check if batch is full so we can send it
        if batch.len() < BATCH && index != header_hashes.len() - 1 {
            continue
        }

        // Request peer headers
        let request = ForkHeadersRequest { headers: batch.clone(), fork_header: proposal.hash };
        channel.0.send(&request).await?;

        // Node waits for response
        let response = response_sub.receive_with_timeout(*channel.1).await?;
        debug!(target: "darkfid::task::handle_reorg", "Peer response: {response:?}");

        // Response sequence must be the same length as the one requested
        if response.headers.len() != batch.len() {
            return Err(Custom(String::from(
                "Peer responded with a different headers sequence length",
            )))
        }

        // Process retrieved headers
        for (peer_header_index, peer_header) in response.headers.iter().enumerate() {
            let peer_header_hash = peer_header.hash();
            debug!(target: "darkfid::task::handle_reorg", "Processing header: {peer_header_hash} - {}", peer_header.height);

            // Validate its the header we requested
            if peer_header_hash != batch[peer_header_index] {
                return Err(Custom(format!(
                    "Peer responded with a differend header: {} - {peer_header_hash}",
                    batch[peer_header_index]
                )))
            }

            // Validate sequence is correct
            if peer_header.previous != previous_hash || peer_header.height != previous_height + 1 {
                return Err(Custom(String::from("Invalid header sequence detected")))
            }

            // Verify header hash and calculate its rank
            let (next_difficulty, target_distance_sq, hash_distance_sq) =
                match header_rank(&module, peer_header) {
                    Ok(tuple) => tuple,
                    Err(PoWInvalidOutHash) => return Err(PoWInvalidOutHash),
                    Err(e) => {
                        return Err(DatabaseError(format!("Computing header rank failed: {e}")))
                    }
                };

            // Update sequence ranking
            targets_rank += target_distance_sq.clone();
            hashes_rank += hash_distance_sq.clone();

            // Update PoW headers module
            module.append(peer_header, &next_difficulty)?;

            // Set previous header
            previous_height = peer_header.height;
            previous_hash = peer_header_hash;
        }

        total_processed += response.headers.len();
        info!(target: "darkfid::task::handle_reorg", "Headers received and verified: {total_processed}/{}", header_hashes.len());

        // Reset batch
        batch = Vec::with_capacity(BATCH);
    }

    // Validate trigger proposal header sequence is correct
    if proposal.block.header.previous != previous_hash ||
        proposal.block.header.height != previous_height + 1
    {
        return Err(Custom(String::from("Invalid header sequence detected")))
    }

    // Verify trigger proposal header hash and calculate its rank
    let (_, target_distance_sq, hash_distance_sq) =
        match header_rank(&module, &proposal.block.header) {
            Ok(tuple) => tuple,
            Err(PoWInvalidOutHash) => return Err(PoWInvalidOutHash),
            Err(e) => return Err(DatabaseError(format!("Computing header rank failed: {e}"))),
        };

    // Update sequence ranking
    targets_rank += target_distance_sq.clone();
    hashes_rank += hash_distance_sq.clone();

    Ok((targets_rank, hashes_rank))
}

/// Auxiliary function to generate provided peer headers hashes fork
/// and its ranking, based on provided last common information.
async fn retrieve_peer_fork(
    // Validator pointer
    validator: &ValidatorPtr,
    // Last common header height, PoW module and difficulty
    last_common_info: (&u32, &PoWModule, &BlockDifficulty),
    // Peer channel and its communications timeout
    channel: (&ChannelPtr, &u64),
    // Peer fork trigger proposal
    proposal: &Proposal,
    // Peer header hashes sequence
    header_hashes: &[HeaderHash],
) -> Result<Fork> {
    // Communication setup
    let response_sub = channel.0.subscribe_msg::<ForkProposalsResponse>().await?;

    // Create a fork from last common height
    let mut peer_fork =
        match Fork::new(validator.consensus.blockchain.clone(), last_common_info.1.clone()).await {
            Ok(f) => f,
            Err(e) => return Err(DatabaseError(format!("Generating peer fork failed: {e}"))),
        };
    peer_fork.targets_rank = last_common_info.2.ranks.targets_rank.clone();
    peer_fork.hashes_rank = last_common_info.2.ranks.hashes_rank.clone();

    // Grab all state inverse diffs after last common height, and add them to the fork
    let inverse_diffs =
        match validator.blockchain.blocks.get_state_inverse_diffs_after(*last_common_info.0) {
            Ok(i) => i,
            Err(e) => {
                return Err(DatabaseError(format!("Retrieving state inverse diffs failed: {e}")))
            }
        };
    for inverse_diff in inverse_diffs.iter().rev() {
        let result =
            peer_fork.overlay.lock().unwrap().overlay.lock().unwrap().add_diff(inverse_diff);
        if let Err(e) = result {
            return Err(DatabaseError(format!("Applying state inverse diff failed: {e}")))
        }
    }

    // Grab current overlay diff and use it as the first diff of the
    // peer fork, so all consecutive diffs represent just the proposal
    // changes.
    let diff = peer_fork.overlay.lock().unwrap().overlay.lock().unwrap().diff(&[]);
    let diff = match diff {
        Ok(d) => d,
        Err(e) => {
            return Err(DatabaseError(format!("Generate full state inverse diff failed: {e}")))
        }
    };
    peer_fork.diffs = vec![diff];

    // Retrieve the proposals of the hashes sequence, in batches
    info!(target: "darkfid::task::handle_reorg", "Peer sequence ranks higher than our current best fork, retrieving {} proposals from peer...", header_hashes.len());
    let mut batch = Vec::with_capacity(BATCH);
    let mut total_processed = 0;
    for (index, hash) in header_hashes.iter().enumerate() {
        // Add hash in batch sequence
        batch.push(*hash);

        // Check if batch is full so we can send it
        if batch.len() < BATCH && index != header_hashes.len() - 1 {
            continue
        }

        // Request peer proposals
        let request = ForkProposalsRequest { headers: batch.clone(), fork_header: proposal.hash };
        channel.0.send(&request).await?;

        // Node waits for response
        let response = response_sub.receive_with_timeout(*channel.1).await?;
        debug!(target: "darkfid::task::handle_reorg", "Peer response: {response:?}");

        // Response sequence must be the same length as the one requested
        if response.proposals.len() != batch.len() {
            return Err(Custom(String::from(
                "Peer responded with a different proposals sequence length",
            )))
        }

        // Process retrieved proposal
        for (peer_proposal_index, peer_proposal) in response.proposals.iter().enumerate() {
            info!(target: "darkfid::task::handle_reorg", "Processing proposal: {} - {}", peer_proposal.hash, peer_proposal.block.header.height);

            // Validate its the proposal we requested
            if peer_proposal.hash != batch[peer_proposal_index] {
                return Err(Custom(format!(
                    "Peer responded with a differend proposal: {} - {}",
                    batch[peer_proposal_index], peer_proposal.hash
                )))
            }

            // Verify proposal
            verify_fork_proposal(&mut peer_fork, peer_proposal, validator.verify_fees).await?;

            // Append proposal
            peer_fork.append_proposal(peer_proposal).await?;
        }

        total_processed += response.proposals.len();
        info!(target: "darkfid::task::handle_reorg", "Proposals received and verified: {total_processed}/{}", header_hashes.len());

        // Reset batch
        batch = Vec::with_capacity(BATCH);
    }

    // Verify trigger proposal
    verify_fork_proposal(&mut peer_fork, proposal, validator.verify_fees).await?;

    // Append trigger proposal
    peer_fork.append_proposal(proposal).await?;

    // Remove the reorg diff from the fork
    peer_fork.diffs.remove(0);

    Ok(peer_fork)
}
