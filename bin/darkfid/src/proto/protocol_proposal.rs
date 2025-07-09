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

use async_trait::async_trait;
use smol::{channel::Sender, lock::RwLock};
use tinyjson::JsonValue;
use tracing::{debug, error};

use darkfi::{
    impl_p2p_message,
    net::{
        metering::MeteringConfiguration,
        protocol::protocol_generic::{
            ProtocolGenericAction, ProtocolGenericHandler, ProtocolGenericHandlerPtr,
        },
        session::SESSION_DEFAULT,
        Message, P2pPtr,
    },
    rpc::jsonrpc::JsonSubscriber,
    system::{ExecutorPtr, StoppableTask, StoppableTaskPtr},
    util::{encoding::base64, time::NanoTimestamp},
    validator::{consensus::Proposal, ValidatorPtr},
    Error, Result,
};
use darkfi_serial::{serialize_async, SerialDecodable, SerialEncodable};

use crate::task::handle_unknown_proposals;

/// Auxiliary [`Proposal`] wrapper structure used for messaging.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ProposalMessage(pub Proposal);

// TODO: Fine tune
// Since messages are asynchronous we will define loose rules to prevent spamming.
// Each message score will be 1, with a threshold of 50 and expiry time of 5.
// We are not limiting `Proposal` size.
impl_p2p_message!(
    ProposalMessage,
    "proposal",
    0,
    1,
    MeteringConfiguration {
        threshold: 50,
        sleep_step: 500,
        expiry_time: NanoTimestamp::from_secs(5),
    }
);

/// Atomic pointer to the `ProtocolProposal` handler.
pub type ProtocolProposalHandlerPtr = Arc<ProtocolProposalHandler>;

/// Handler managing [`Proposal`] messages, over a generic P2P protocol.
pub struct ProtocolProposalHandler {
    /// The generic handler for [`Proposal`] messages.
    proposals_handler: ProtocolGenericHandlerPtr<ProposalMessage, ProposalMessage>,
    /// Unknown proposals queue to be checked for reorg.
    unknown_proposals: Arc<RwLock<HashSet<[u8; 32]>>>,
    /// Handler background task to process unknown proposals queue.
    unknown_proposals_handler: StoppableTaskPtr,
}

impl ProtocolProposalHandler {
    /// Initialize a generic prototocol handler for [`Proposal`] messages
    /// and registers it to the provided P2P network, using the default session flag.
    pub async fn init(p2p: &P2pPtr) -> ProtocolProposalHandlerPtr {
        debug!(
            target: "darkfid::proto::protocol_proposal::init",
            "Adding ProtocolProposal to the protocol registry"
        );

        let proposals_handler =
            ProtocolGenericHandler::new(p2p, "ProtocolProposal", SESSION_DEFAULT).await;
        let unknown_proposals = Arc::new(RwLock::new(HashSet::new()));
        let unknown_proposals_handler = StoppableTask::new();

        Arc::new(Self { proposals_handler, unknown_proposals, unknown_proposals_handler })
    }

    /// Start the `ProtocolProposal` background task.
    pub async fn start(
        &self,
        executor: &ExecutorPtr,
        validator: &ValidatorPtr,
        p2p: &P2pPtr,
        proposals_sub: JsonSubscriber,
        blocks_sub: JsonSubscriber,
    ) -> Result<()> {
        debug!(
            target: "darkfid::proto::protocol_proposal::start",
            "Starting ProtocolProposal handler task..."
        );

        // Generate the message queue smol channel
        let (sender, receiver) = smol::channel::unbounded::<(Proposal, u32)>();

        // Start the unkown proposals handler task
        self.unknown_proposals_handler.clone().start(
            handle_unknown_proposals(receiver, self.unknown_proposals.clone(), validator.clone(), p2p.clone(), proposals_sub.clone(), blocks_sub),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid::proto::protocol_proposal::start", "Failed starting unknown proposals handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        // Start the proposals handler task
        self.proposals_handler.task.clone().start(
            handle_receive_proposal(self.proposals_handler.clone(), sender, self.unknown_proposals.clone(), validator.clone(), proposals_sub),
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

    /// Stop the `ProtocolProposal` background tasks.
    pub async fn stop(&self) {
        debug!(target: "darkfid::proto::protocol_proposal::stop", "Terminating ProtocolProposal handler task...");
        self.unknown_proposals_handler.stop().await;
        self.proposals_handler.task.stop().await;
        let mut unknown_proposals = self.unknown_proposals.write().await;
        *unknown_proposals = HashSet::new();
        drop(unknown_proposals);
        debug!(target: "darkfid::proto::protocol_proposal::stop", "ProtocolProposal handler task terminated!");
    }
}

/// Background handler function for ProtocolProposal.
async fn handle_receive_proposal(
    handler: ProtocolGenericHandlerPtr<ProposalMessage, ProposalMessage>,
    sender: Sender<(Proposal, u32)>,
    unknown_proposals: Arc<RwLock<HashSet<[u8; 32]>>>,
    validator: ValidatorPtr,
    proposals_sub: JsonSubscriber,
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

                // Notify proposals subscriber
                let enc_prop = JsonValue::String(base64::encode(&serialize_async(&proposal).await));
                proposals_sub.notify(vec![enc_prop].into()).await;

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

        // Check if we already have the unknown proposal record in our
        // queue.
        let mut lock = unknown_proposals.write().await;
        if lock.contains(proposal.0.hash.inner()) {
            debug!(
                target: "darkfid::proto::protocol_proposal::handle_receive_proposal",
                "Proposal {} is already in our unknown proposals queue.",
                proposal.0.hash,
            );
            drop(lock);
            continue
        }

        // Insert new record in our queue
        lock.insert(proposal.0.hash.0);
        drop(lock);

        // Notify the unknown proposals handler task
        if let Err(e) = sender.send((proposal.0, channel)).await {
            debug!(
                target: "darkfid::proto::protocol_proposal::handle_receive_proposal",
                "Channel {channel} send fail: {e}"
            );
        };
    }
}
