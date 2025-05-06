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

use log::{debug, error, info, warn};
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
pub async fn handle_unknown_proposal(
    validator: ValidatorPtr,
    p2p: P2pPtr,
    proposals_sub: JsonSubscriber,
    blocks_sub: JsonSubscriber,
    channel: u32,
    proposal: Proposal,
) -> Result<()> {
    // If proposal fork chain was not found, we ask our peer for its sequence
    debug!(target: "darkfid::task::handle_unknown_proposal", "Asking peer for fork sequence");
    let Some(channel) = p2p.get_channel(channel) else {
        error!(target: "darkfid::task::handle_unknown_proposal", "Channel {channel} wasn't found.");
        return Ok(())
    };

    // Communication setup
    let Ok(response_sub) = channel.subscribe_msg::<ForkSyncResponse>().await else {
        error!(target: "darkfid::task::handle_unknown_proposal", "Failure during `ForkSyncResponse` communication setup with peer: {channel:?}");
        return Ok(())
    };

    // Grab last known block to create the request and execute it
    let last = match validator.blockchain.last() {
        Ok(l) => l,
        Err(e) => {
            debug!(target: "darkfid::task::handle_unknown_proposal", "Blockchain last retriaval failed: {e}");
            return Ok(())
        }
    };
    let request = ForkSyncRequest { tip: last.1, fork_tip: Some(proposal.hash) };
    if let Err(e) = channel.send(&request).await {
        debug!(target: "darkfid::task::handle_unknown_proposal", "Channel send failed: {e}");
        return Ok(())
    };

    // Node waits for response
    let response = match response_sub
        .receive_with_timeout(p2p.settings().read().await.outbound_connect_timeout)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            debug!(target: "darkfid::task::handle_unknown_proposal", "Asking peer for fork sequence failed: {e}");
            return Ok(())
        }
    };
    debug!(target: "darkfid::task::handle_unknown_proposal", "Peer response: {response:?}");

    // Verify and store retrieved proposals
    debug!(target: "darkfid::task::handle_unknown_proposal", "Processing received proposals");

    // Response should not be empty
    if response.proposals.is_empty() {
        warn!(target: "darkfid::task::handle_unknown_proposal", "Peer responded with empty sequence, node might be out of sync!");
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
                error!(
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

    Ok(())
}

// TODO; If a reorg trigger is erroneous, disconnect from peer.
/// Auxiliary function to handle a potential reorg.
/// We first find our last common block with the peer,
/// then grab the header sequence from that block until
/// the proposal and check if it ranks higher than our
/// current best ranking fork, to perform a reorg.
async fn handle_reorg(
    validator: ValidatorPtr,
    p2p: P2pPtr,
    proposals_sub: JsonSubscriber,
    blocks_sub: JsonSubscriber,
    channel: ChannelPtr,
    proposal: Proposal,
) -> Result<()> {
    info!(target: "darkfid::task::handle_reorg", "Checking for potential reorg from proposal {} - {} by peer: {channel:?}", proposal.hash, proposal.block.header.height);

    // Check if genesis proposal was provided
    if proposal.block.header.height == 0 {
        info!(target: "darkfid::task::handle_reorg", "Peer send a genesis proposal, skipping...");
        return Ok(())
    }

    // Communication setup
    let Ok(response_sub) = channel.subscribe_msg::<ForkHeaderHashResponse>().await else {
        error!(target: "darkfid::task::handle_reorg", "Failure during `ForkHeaderHashResponse` communication setup with peer: {channel:?}");
        return Ok(())
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
            return Ok(())
        };

        // Node waits for response
        let response = match response_sub
            .receive_with_timeout(p2p.settings().read().await.outbound_connect_timeout)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                debug!(target: "darkfid::task::handle_reorg", "Asking peer for header hash failed: {e}");
                return Ok(())
            }
        };
        debug!(target: "darkfid::task::handle_reorg", "Peer response: {response:?}");

        // Check if peer returned a header
        let Some(peer_header) = response.fork_header else {
            info!(target: "darkfid::task::handle_reorg", "Peer responded with an empty header");
            return Ok(())
        };

        // Check if we know this header
        match validator.blockchain.blocks.get_order(&[height], false)?[0] {
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
        info!(target: "darkfid::task::handle_reorg", "No headers to process, skipping...");
        return Ok(())
    }

    // Communication setup
    let Ok(response_sub) = channel.subscribe_msg::<ForkHeadersResponse>().await else {
        error!(target: "darkfid::task::handle_reorg", "Failure during `ForkHeadersResponse` communication setup with peer: {channel:?}");
        return Ok(())
    };

    // Grab last common height ranks
    let last_common_height = previous_height;
    let last_difficulty = match previous_height {
        0 => BlockDifficulty::genesis(validator.blockchain.genesis_block()?.header.timestamp),
        _ => validator.blockchain.blocks.get_difficulty(&[last_common_height], true)?[0]
            .clone()
            .unwrap(),
    };

    // Create a new PoW from last common height
    let module = PoWModule::new(
        validator.consensus.blockchain.clone(),
        validator.consensus.module.read().await.target,
        validator.consensus.module.read().await.fixed_difficulty.clone(),
        Some(last_common_height + 1),
    )?;

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
            return Ok(())
        };

        // Node waits for response
        let response = match response_sub
            .receive_with_timeout(p2p.settings().read().await.outbound_connect_timeout)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                debug!(target: "darkfid::task::handle_reorg", "Asking peer for headers sequence failed: {e}");
                return Ok(())
            }
        };
        debug!(target: "darkfid::task::handle_reorg", "Peer response: {response:?}");

        // Response sequence must be the same length as the one requested
        if response.headers.len() != batch.len() {
            error!(target: "darkfid::task::handle_reorg", "Peer responded with a different headers sequence length");
            return Ok(())
        }

        // Process retrieved headers
        for (peer_header_index, peer_header) in response.headers.iter().enumerate() {
            let peer_header_hash = peer_header.hash();
            info!(target: "darkfid::task::handle_reorg", "Processing header: {peer_header_hash} - {}", peer_header.height);

            // Validate its the header we requested
            if peer_header_hash != batch[peer_header_index] {
                error!(target: "darkfid::task::handle_reorg", "Peer responded with a differend header: {} - {peer_header_hash}", batch[peer_header_index]);
                return Ok(())
            }

            // Validate sequence is correct
            if peer_header.previous != previous_hash || peer_header.height != previous_height + 1 {
                error!(target: "darkfid::task::handle_reorg", "Invalid header sequence detected");
                return Ok(())
            }

            // Grab next mine target and difficulty
            let (next_target, next_difficulty) =
                headers_module.next_mine_target_and_difficulty()?;

            // Verify header hash and calculate its rank
            let (target_distance_sq, hash_distance_sq) = match header_rank(
                peer_header,
                &next_target,
            ) {
                Ok(distances) => distances,
                Err(e) => {
                    error!(target: "darkfid::task::handle_reorg", "Invalid header hash detected: {e}");
                    return Ok(())
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
    let best_fork = &forks[best_fork_index(&forks)?];
    if targets_rank < best_fork.targets_rank ||
        (targets_rank == best_fork.targets_rank && hashes_rank <= best_fork.hashes_rank)
    {
        info!(target: "darkfid::task::handle_reorg", "Peer sequence ranks lower than our current best fork, skipping...");
        drop(forks);
        return Ok(())
    }
    drop(forks);

    // Communication setup
    let Ok(response_sub) = channel.subscribe_msg::<ForkProposalsResponse>().await else {
        error!(target: "darkfid::task::handle_reorg", "Failure during `ForkProposalsResponse` communication setup with peer: {channel:?}");
        return Ok(())
    };

    // Create a fork from last common height
    let mut peer_fork = Fork::new(validator.consensus.blockchain.clone(), module).await?;
    peer_fork.targets_rank = last_difficulty.ranks.targets_rank.clone();
    peer_fork.hashes_rank = last_difficulty.ranks.hashes_rank.clone();

    // Grab all state inverse diffs after last common height, and add them to the fork
    let inverse_diffs =
        validator.blockchain.blocks.get_state_inverse_diffs_after(last_common_height)?;
    for inverse_diff in inverse_diffs.iter().rev() {
        peer_fork.overlay.lock().unwrap().overlay.lock().unwrap().add_diff(inverse_diff)?;
    }

    // Rebuild fork contracts states monotree
    peer_fork.compute_monotree()?;

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
            return Ok(())
        };

        // Node waits for response
        let response = match response_sub
            .receive_with_timeout(p2p.settings().read().await.outbound_connect_timeout)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                debug!(target: "darkfid::task::handle_reorg", "Asking peer for proposals sequence failed: {e}");
                return Ok(())
            }
        };
        debug!(target: "darkfid::task::handle_reorg", "Peer response: {response:?}");

        // Response sequence must be the same length as the one requested
        if response.proposals.len() != batch.len() {
            error!(target: "darkfid::task::handle_reorg", "Peer responded with a different proposals sequence length");
            return Ok(())
        }

        // Process retrieved proposal
        for (peer_proposal_index, peer_proposal) in response.proposals.iter().enumerate() {
            info!(target: "darkfid::task::handle_reorg", "Processing proposal: {} - {}", peer_proposal.hash, peer_proposal.block.header.height);

            // Validate its the proposal we requested
            if peer_proposal.hash != batch[peer_proposal_index] {
                error!(target: "darkfid::task::handle_reorg", "Peer responded with a differend proposal: {} - {}", batch[peer_proposal_index], peer_proposal.hash);
                return Ok(())
            }

            // Verify proposal
            if let Err(e) =
                verify_fork_proposal(&mut peer_fork, peer_proposal, validator.verify_fees).await
            {
                error!(target: "darkfid::task::handle_reorg", "Verify fork proposal failed: {e}");
                return Ok(())
            }

            // Append proposal
            if let Err(e) = peer_fork.append_proposal(peer_proposal).await {
                error!(target: "darkfid::task::handle_reorg", "Appending proposal failed: {e}");
                return Ok(())
            }
        }

        total_processed += response.proposals.len();
        info!(target: "darkfid::task::handle_reorg", "Proposals received and verified: {total_processed}/{}", peer_header_hashes.len());

        // Reset batch
        batch = Vec::with_capacity(BATCH);
    }

    // Verify trigger proposal
    if let Err(e) = verify_fork_proposal(&mut peer_fork, &proposal, validator.verify_fees).await {
        error!(target: "darkfid::task::handle_reorg", "Verify proposal failed: {e}");
        return Ok(())
    }

    // Append trigger proposal
    if let Err(e) = peer_fork.append_proposal(&proposal).await {
        error!(target: "darkfid::task::handle_reorg", "Appending proposal failed: {e}");
        return Ok(())
    }

    // Check if the peer fork ranks higher than our current best fork
    let mut forks = validator.consensus.forks.write().await;
    let best_fork = &forks[best_fork_index(&forks)?];
    if peer_fork.targets_rank < best_fork.targets_rank ||
        (peer_fork.targets_rank == best_fork.targets_rank &&
            peer_fork.hashes_rank <= best_fork.hashes_rank)
    {
        info!(target: "darkfid::task::handle_reorg", "Peer fork ranks lower than our current best fork, skipping...");
        drop(forks);
        return Ok(())
    }

    // Execute the reorg
    info!(target: "darkfid::task::handle_reorg", "Peer fork ranks higher than our current best fork, executing reorg...");
    *forks = vec![peer_fork];
    drop(forks);

    // Check if we can confirm anything and broadcast them
    let confirmed = match validator.confirmation().await {
        Ok(f) => f,
        Err(e) => {
            error!(target: "darkfid::task::handle_reorg", "Confirmation failed: {e}");
            return Ok(())
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
    let enc_prop = JsonValue::String(base64::encode(&serialize_async(&proposal).await));
    proposals_sub.notify(vec![enc_prop].into()).await;

    Ok(())
}
