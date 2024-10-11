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

use log::{debug, error, warn};
use tinyjson::JsonValue;

use darkfi::{
    net::P2pPtr,
    rpc::jsonrpc::JsonSubscriber,
    util::encoding::base64,
    validator::{consensus::Proposal, ValidatorPtr},
    Error, Result,
};
use darkfi_serial::serialize_async;

use crate::proto::{ForkSyncRequest, ForkSyncResponse, ProposalMessage};

/// Background task to handle unknown proposals.
pub async fn handle_unknown_proposal(
    validator: ValidatorPtr,
    p2p: P2pPtr,
    subscriber: JsonSubscriber,
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
        return Ok(())
    }

    // Sequence length must correspond to requested height
    if response.proposals.len() as u32 != proposal.block.header.height - last.0 {
        debug!(target: "darkfid::task::handle_unknown_proposal", "Response sequence length is erroneous");
        return Ok(())
    }

    // First proposal must extend canonical
    if response.proposals[0].block.header.previous != last.1 {
        debug!(target: "darkfid::task::handle_unknown_proposal", "Response sequence doesn't extend canonical");
        return Ok(())
    }

    // Last proposal must be the same as the one requested
    if response.proposals.last().unwrap().hash != proposal.hash {
        debug!(target: "darkfid::task::handle_unknown_proposal", "Response sequence doesn't correspond to requested tip");
        return Ok(())
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

        // Notify subscriber
        let enc_prop = JsonValue::String(base64::encode(&serialize_async(proposal).await));
        subscriber.notify(vec![enc_prop].into()).await;
    }

    Ok(())
}
