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
use log::{debug, error};
use smol::lock::RwLock;
use tinyjson::JsonValue;

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

use crate::task::handle_unknown_proposal;

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
    handler: ProtocolGenericHandlerPtr<ProposalMessage, ProposalMessage>,
    /// Background tasks invoked by the handler.
    tasks: Arc<RwLock<HashSet<StoppableTaskPtr>>>,
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
        let tasks = Arc::new(RwLock::new(HashSet::new()));

        Arc::new(Self { handler, tasks })
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

        self.handler.task.clone().start(
            handle_receive_proposal(self.handler.clone(), self.tasks.clone(), validator.clone(), p2p.clone(), proposals_sub, blocks_sub, executor.clone()),
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
        self.handler.task.stop().await;
        let mut tasks = self.tasks.write().await;
        for task in tasks.iter() {
            task.stop().await;
        }
        *tasks = HashSet::new();
        drop(tasks);
        debug!(target: "darkfid::proto::protocol_proposal::stop", "ProtocolProposal handler task terminated!");
    }
}

/// Background handler function for ProtocolProposal.
async fn handle_receive_proposal(
    handler: ProtocolGenericHandlerPtr<ProposalMessage, ProposalMessage>,
    tasks: Arc<RwLock<HashSet<StoppableTaskPtr>>>,
    validator: ValidatorPtr,
    p2p: P2pPtr,
    proposals_sub: JsonSubscriber,
    blocks_sub: JsonSubscriber,
    executor: ExecutorPtr,
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

        // Handle unknown proposal in the background
        let task = StoppableTask::new();
        let _tasks = tasks.clone();
        let _task = task.clone();
        task.clone().start(
            handle_unknown_proposal(validator.clone(), p2p.clone(), proposals_sub.clone(), blocks_sub.clone(), channel, proposal.0),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { _tasks.write().await.remove(&_task); }
                    Err(e) => error!(target: "darkfid::proto::protocol_proposal::start", "Failed starting unknown proposal handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );
        tasks.write().await.insert(task);
    }
}
