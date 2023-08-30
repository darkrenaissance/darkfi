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
use log::{debug, error, trace};
use smol::Executor;
use url::Url;

use crate::{
    consensus::{BlockProposal, ValidatorStatePtr},
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};

pub struct ProtocolProposal {
    proposal_sub: MessageSubscription<BlockProposal>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
    p2p: P2pPtr,
    channel_address: Url,
}

impl ProtocolProposal {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        debug!(target: "consensus::protocol_proposal::init()", "Adding ProtocolProposal to the protocol registry");
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<BlockProposal>().await;

        let proposal_sub = channel.subscribe_msg::<BlockProposal>().await?;

        Ok(Arc::new(Self {
            proposal_sub,
            jobsman: ProtocolJobsManager::new("ProposalProtocol", channel.clone()),
            state,
            p2p,
            channel_address: channel.address().clone(),
        }))
    }

    async fn handle_receive_proposal(self: Arc<Self>) -> Result<()> {
        debug!(target: "consensus::protocol_proposal::handle_receive_proposal()", "START");
        let exclude_list = vec![self.channel_address.clone()];
        loop {
            let proposal = match self.proposal_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "consensus::protocol_proposal::handle_receive_proposal()",
                        "recv fail: {}",
                        e
                    );
                    continue
                }
            };

            debug!(
                target: "consensus::protocol_proposal::handle_receive_proposal()",
                "recv: {}", proposal);
            trace!(
                target: "consensus::protocol_proposal::handle_receive_proposal()",
                "Full proposal: {:?}",
                proposal
            );

            let proposal_copy = (*proposal).clone();

            // Verify we have the proposal already
            let mut lock = self.state.write().await;
            if lock.consensus.proposal_exists(&proposal_copy.hash) {
                debug!(
                    target: "consensus::protocol_proposal::handle_receive_proposal()",
                    "Proposal already received."
                );
                continue
            }

            match lock.receive_proposal(&proposal_copy, None).await {
                Ok(broadcast) => {
                    if broadcast {
                        // Broadcast proposal to rest of nodes
                        self.p2p.broadcast_with_exclude(&proposal_copy, &exclude_list).await;
                    }
                }
                Err(e) => {
                    error!(
                        target: "consensus::protocol_proposal::handle_receive_proposal()",
                        "receive_proposal error: {}",
                        e
                    );
                    continue
                }
            }
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolProposal {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "consensus::protocol_proposal::start()", "START");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_proposal(), executor.clone()).await;
        debug!(target: "consensus::protocol_proposal::start()", "END");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolProposal"
    }
}
