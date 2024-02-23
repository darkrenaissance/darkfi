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
use log::debug;
use smol::Executor;
use tinyjson::JsonValue;

use darkfi::{
    impl_p2p_message,
    net::{
        ChannelPtr, Message, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    rpc::jsonrpc::JsonSubscriber,
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

pub struct ProtocolProposal {
    proposal_sub: MessageSubscription<ProposalMessage>,
    proposals_response_sub: MessageSubscription<ForkSyncResponse>,
    jobsman: ProtocolJobsManagerPtr,
    validator: ValidatorPtr,
    p2p: P2pPtr,
    channel: ChannelPtr,
    subscriber: JsonSubscriber,
    miner: bool,
}

impl ProtocolProposal {
    pub async fn init(
        channel: ChannelPtr,
        validator: ValidatorPtr,
        p2p: P2pPtr,
        subscriber: JsonSubscriber,
        miner: bool,
    ) -> Result<ProtocolBasePtr> {
        debug!(
            target: "darkfid::proto::protocol_proposal::init",
            "Adding ProtocolProposal to the protocol registry"
        );
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<ProposalMessage>().await;
        msg_subsystem.add_dispatch::<ForkSyncRequest>().await;
        msg_subsystem.add_dispatch::<ForkSyncResponse>().await;

        let proposal_sub = channel.subscribe_msg::<ProposalMessage>().await?;
        let proposals_response_sub = channel.subscribe_msg::<ForkSyncResponse>().await?;

        Ok(Arc::new(Self {
            proposal_sub,
            proposals_response_sub,
            jobsman: ProtocolJobsManager::new("ProposalProtocol", channel.clone()),
            validator,
            p2p,
            channel,
            subscriber,
            miner,
        }))
    }

    async fn handle_receive_proposal(self: Arc<Self>) -> Result<()> {
        debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "START");
        let exclude_list = vec![self.channel.address().clone()];
        loop {
            let proposal = match self.proposal_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "darkfid::proto::protocol_proposal::handle_receive_proposal",
                        "recv fail: {}",
                        e
                    );
                    continue
                }
            };

            // Check if node has finished syncing its blockchain
            if !*self.validator.synced.read().await {
                debug!(
                    target: "darkfid::proto::protocol_proposal::handle_receive_proposal",
                    "Node still syncing blockchain, skipping..."
                );
                continue
            }

            // Check if node is connected to the miners network {
            if self.miner {
                debug!(
                    target: "darkfid::proto::protocol_proposal::handle_receive_proposal",
                    "Node is connected to the miners network, skipping..."
                );
                continue
            }

            let proposal_copy = (*proposal).clone();

            match self.validator.consensus.append_proposal(&proposal_copy.0).await {
                Ok(()) => {
                    self.p2p.broadcast_with_exclude(&proposal_copy, &exclude_list).await;
                    let enc_prop =
                        JsonValue::String(base64::encode(&serialize_async(&proposal_copy).await));
                    self.subscriber.notify(vec![enc_prop].into()).await;
                    continue
                }
                Err(e) => {
                    debug!(
                        target: "darkfid::proto::protocol_proposal::handle_receive_proposal",
                        "append_proposal fail: {}",
                        e
                    );

                    match e {
                        Error::ExtendedChainIndexNotFound => { /* Do nothing */ }
                        _ => continue,
                    }
                }
            };

            // If proposal fork chain was not found, we ask our peer for its sequence
            debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Asking peer for fork sequence");
            let last = self.validator.blockchain.last()?;
            let request = ForkSyncRequest { tip: last.1, fork_tip: Some(proposal_copy.0.hash) };
            self.channel.send(&request).await?;

            // TODO: add a timeout here to retry
            // Node waits for response
            let response = self.proposals_response_sub.receive().await?;

            // Verify and store retrieved proposals
            debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Processing received proposals");

            // Response should not be empty
            if response.proposals.is_empty() {
                debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Peer responded with empty sequence");
                continue
            }

            // Sequence length must correspond to requested height
            if response.proposals.len() as u64 != proposal_copy.0.block.header.height - last.0 {
                debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Response sequence length is erroneous");
                continue
            }

            // First proposal must extend canonical
            if response.proposals[0].block.header.previous != last.1 {
                debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Response sequence doesn't extend canonical");
                continue
            }

            // Last proposal must be the same as the one requested
            if response.proposals.last().unwrap().hash != proposal_copy.0.hash {
                debug!(target: "darkfid::proto::protocol_proposal::handle_receive_proposal", "Response sequence doesn't correspond to requested tip");
                continue
            }

            for proposal in &response.proposals {
                self.validator.consensus.append_proposal(proposal).await?;
                // Notify subscriber
                let enc_prop = JsonValue::String(base64::encode(&serialize_async(proposal).await));
                self.subscriber.notify(vec![enc_prop].into()).await;
            }
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolProposal {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "darkfid::proto::protocol_proposal::start", "START");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_proposal(), executor.clone()).await;
        debug!(target: "darkfid::proto::protocol_proposal::start", "END");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolProposal"
    }
}
