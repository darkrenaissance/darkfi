/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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
use url::Url;

use darkfi::{
    impl_p2p_message,
    net::{
        ChannelPtr, Message, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    rpc::jsonrpc::JsonSubscriber,
    util::encoding::base64,
    validator::{consensus::Proposal, ValidatorPtr},
    Result,
};
use darkfi_serial::{serialize, SerialDecodable, SerialEncodable};

/// Auxiliary [`Proposal`] wrapper structure used for messaging.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
struct ProposalMessage(Proposal);

impl_p2p_message!(ProposalMessage, "proposal");

pub struct ProtocolProposal {
    proposal_sub: MessageSubscription<ProposalMessage>,
    jobsman: ProtocolJobsManagerPtr,
    validator: ValidatorPtr,
    p2p: P2pPtr,
    channel_address: Url,
    subscriber: JsonSubscriber,
}

impl ProtocolProposal {
    pub async fn init(
        channel: ChannelPtr,
        validator: ValidatorPtr,
        p2p: P2pPtr,
        subscriber: JsonSubscriber,
    ) -> Result<ProtocolBasePtr> {
        debug!(
            target: "validator::protocol_proposal::init",
            "Adding ProtocolProposal to the protocol registry"
        );
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<ProposalMessage>().await;

        let proposal_sub = channel.subscribe_msg::<ProposalMessage>().await?;

        Ok(Arc::new(Self {
            proposal_sub,
            jobsman: ProtocolJobsManager::new("ProposalProtocol", channel.clone()),
            validator,
            p2p,
            channel_address: channel.address().clone(),
            subscriber,
        }))
    }

    async fn handle_receive_proposal(self: Arc<Self>) -> Result<()> {
        debug!(target: "consensus::protocol_proposal::handle_receive_proposal", "START");
        let exclude_list = vec![self.channel_address.clone()];
        loop {
            let proposal = match self.proposal_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "validator::protocol_proposal::handle_receive_proposal",
                        "recv fail: {}",
                        e
                    );
                    continue
                }
            };

            // Check if node has finished syncing its blockchain
            if !self.validator.read().await.synced {
                debug!(
                    target: "validator::protocol_proposal::handle_receive_proposal",
                    "Node still syncing blockchain, skipping..."
                );
                continue
            }

            // Check if node started participating in consensus.
            if !self.validator.read().await.consensus.participating {
                debug!(
                    target: "validator::protocol_proposal::handle_receive_proposal",
                    "Node is not participating in consensus, skipping..."
                );
                continue
            }

            let proposal_copy = (*proposal).clone();

            match self.validator.write().await.consensus.append_proposal(&proposal_copy.0).await {
                Ok(()) => {
                    self.p2p.broadcast_with_exclude(&proposal_copy, &exclude_list).await;
                    let enc_prop = JsonValue::String(base64::encode(&serialize(&proposal_copy)));
                    self.subscriber.notify(vec![enc_prop]).await;
                }
                Err(e) => {
                    debug!(
                        target: "validator::protocol_proposal::handle_receive_proposal",
                        "append_proposal fail: {}",
                        e
                    );
                }
            };
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolProposal {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "validator::protocol_proposal::start", "START");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_proposal(), executor.clone()).await;
        debug!(target: "validator::protocol_proposal::start", "END");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolProposal"
    }
}
