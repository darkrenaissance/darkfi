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

use std::sync::Arc;

use async_trait::async_trait;
use log::{debug, error, warn};
use tinyjson::JsonValue;

use darkfi::{
    impl_p2p_message,
    net::{
        protocol::protocol_generic::{
            ProtocolGenericAction, ProtocolGenericHandler, ProtocolGenericHandlerPtr,
        },
        session::SESSION_DEFAULT,
        Message, P2pPtr,
    },
    rpc::jsonrpc::JsonSubscriber,
    system::ExecutorPtr,
    util::encoding::base64,
    validator::{consensus::Proposal, ValidatorPtr},
    Error, Result,
};
use darkfi_serial::{serialize_async, SerialDecodable, SerialEncodable};

use crate::proto::{ForkSyncRequest, ForkSyncResponse};

/// Auxiliary [`Proposal`] wrapper structure used for messaging.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ProposalMessage(pub Proposal);

impl_p2p_message!(ProposalMessage, "proposal");

/// Atomic pointer to the `ProtocolProposal` handler.
pub type ProtocolProposalHandlerPtr = Arc<ProtocolProposalHandler>;

/// Handler managing [`Proposal`] messages, over a generic P2P protocol.
pub struct ProtocolProposalHandler {
    /// The generic handler for [`Proposal`] messages.
    handler: ProtocolGenericHandlerPtr<ProposalMessage, ProposalMessage>,
}

impl ProtocolProposalHandler {
    /// Initialize a generic prototocol handler for [`Proposal`] messages
    /// and registers it to the provided P2P network, using the default session flag.
    pub async fn init(p2p: &P2pPtr) -> ProtocolProposalHandlerPtr {
        debug!(
            target: "darkfid::proto::protocol_proposal::init",
            "Adding ProtocolProposal to the protocol registry"
        );

        let handler = ProtocolGenericHandler::new(p2p, "ProtocolProposal", SESSION_DEFAULT).await;

        Arc::new(Self { handler })
    }

    /// Start the `ProtocolProposal` background task.
    pub async fn start(
        &self,
        executor: &ExecutorPtr,
        validator: &ValidatorPtr,
        p2p: &P2pPtr,
        subscriber: JsonSubscriber,
    ) -> Result<()> {
        debug!(
            target: "darkfid::proto::protocol_proposal::start",
            "Starting ProtocolProposal handler task..."
        );

        self.handler.task.clone().start(
            handle_receive_proposal(self.handler.clone(), validator.clone(), p2p.clone(), subscriber),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid::proto::protocol_proposal::start", "Failed starting ProtocolProposal handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        debug!(
            target: "darkfid::proto::protocol_proposal::start",
            "ProtocolProposal handler task started!"
        );

        Ok(())
    }

    /// Stop the `ProtocolProposal` background task.
    pub async fn stop(&self) {
        debug!(target: "darkfid::proto::protocol_proposal::stop", "Terminating ProtocolProposal handler task...");
        self.handler.task.stop().await;
        debug!(target: "darkfid::proto::protocol_proposal::stop", "ProtocolProposal handler task terminated!");
    }
}

/// Background handler function for ProtocolProposal.
async fn handle_receive_proposal(
    handler: ProtocolGenericHandlerPtr<ProposalMessage, ProposalMessage>,
    validator: ValidatorPtr,
    p2p: P2pPtr,
    subscriber: JsonSubscriber,
) -> Result<()> {
    debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "START");
    loop {
        // Wait for a new proposal message
        let (channel, proposal) = match handler.receiver.recv().await {
            Ok(r) => r,
            Err(e) => {
                debug!(
                    target: "darkfid::proto::protocol_proposal::handle_receive_proposal",
                    "recv fail: {e}"
                );
                continue
            }
        };

        // Check if node has finished syncing its blockchain
        if !*validator.synced.read().await {
            debug!(
                target: "darkfid::proto::protocol_proposal::handle_receive_proposal",
                "Node still syncing blockchain, skipping..."
            );
            handler.send_action(channel, ProtocolGenericAction::Skip).await;
            continue
        }

        // Append proposal
        match validator.append_proposal(&proposal.0).await {
            Ok(()) => {
                // Signal handler to broadcast the valid proposal to rest nodes
                handler.send_action(channel, ProtocolGenericAction::Broadcast).await;

                // Notify subscriber
                let enc_prop = JsonValue::String(base64::encode(&serialize_async(&proposal).await));
                subscriber.notify(vec![enc_prop].into()).await;

                continue
            }
            Err(e) => {
                debug!(
                    target: "darkfid::proto::protocol_proposal::handle_receive_proposal",
                    "append_proposal fail: {e}",
                );

                handler.send_action(channel, ProtocolGenericAction::Skip).await;

                match e {
                    Error::ExtendedChainIndexNotFound => { /* Do nothing */ }
                    _ => continue,
                }
            }
        };

        // If proposal fork chain was not found, we ask our peer for its sequence
        debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Asking peer for fork sequence");
        let Some(channel) = p2p.get_channel(channel) else {
            error!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Channel {channel} wasn't found.");
            continue
        };

        // Communication setup
        let Ok(response_sub) = channel.subscribe_msg::<ForkSyncResponse>().await else {
            error!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Failure during `ForkSyncResponse` communication setup with peer: {channel:?}");
            continue
        };

        // Grab last known block to create the request and execute it
        let last = match validator.blockchain.last() {
            Ok(l) => l,
            Err(e) => {
                debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Blockchain last retriaval failed: {e}");
                continue
            }
        };
        let request = ForkSyncRequest { tip: last.1, fork_tip: Some(proposal.0.hash) };
        if let Err(e) = channel.send(&request).await {
            debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Channel send failed: {e}");
            continue
        };

        // Node waits for response
        let response = match response_sub
            .receive_with_timeout(p2p.settings().read().await.outbound_connect_timeout)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Asking peer for fork sequence failed: {e}");
                continue
            }
        };
        debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Peer response: {response:?}");

        // Verify and store retrieved proposals
        debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Processing received proposals");

        // Response should not be empty
        if response.proposals.is_empty() {
            warn!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Peer responded with empty sequence, node might be out of sync!");
            continue
        }

        // Sequence length must correspond to requested height
        if response.proposals.len() as u32 != proposal.0.block.header.height - last.0 {
            debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Response sequence length is erroneous");
            continue
        }

        // First proposal must extend canonical
        if response.proposals[0].block.header.previous != last.1 {
            debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Response sequence doesn't extend canonical");
            continue
        }

        // Last proposal must be the same as the one requested
        if response.proposals.last().unwrap().hash != proposal.0.hash {
            debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Response sequence doesn't correspond to requested tip");
            continue
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
                        target: "darkfid::proto::protocol_proposal::handle_receive_proposal",
                        "Error while appending response proposal: {e}"
                    );
                }
            };

            // Broadcast proposal to rest nodes
            let message = ProposalMessage(proposal.clone());
            p2p.broadcast_with_exclude(&message, &[channel.address().clone()]).await;

            // Notify subscriber
            let enc_prop = JsonValue::String(base64::encode(&serialize_async(proposal).await));
            subscriber.notify(vec![enc_prop].into()).await;
        }
    }
}
