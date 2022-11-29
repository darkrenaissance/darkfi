/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use async_std::sync::Arc;
use async_trait::async_trait;
use log::{debug, error};
use smol::Executor;

use crate::{
    consensus::{
        state::{ConsensusRequest, ConsensusResponse},
        ValidatorStatePtr,
    },
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};

pub struct ProtocolSyncConsensus {
    channel: ChannelPtr,
    request_sub: MessageSubscription<ConsensusRequest>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
}

impl ProtocolSyncConsensus {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        _p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<ConsensusRequest>().await;

        let request_sub = channel.subscribe_msg::<ConsensusRequest>().await?;

        Ok(Arc::new(Self {
            channel: channel.clone(),
            request_sub,
            jobsman: ProtocolJobsManager::new("SyncConsensusProtocol", channel),
            state,
        }))
    }

    async fn handle_receive_request(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolSyncConsensus::handle_receive_request() [START]");
        loop {
            let order = match self.request_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!("ProtocolSyncConsensus::handle_receive_request() recv fail: {}", e);
                    continue
                }
            };

            debug!("ProtocolSyncConsensuss::handle_receive_request() received {:?}", order);

            // Extra validations can be added here.
            let lock = self.state.read().await;
            let offset = lock.consensus.offset;
            let proposals = lock.consensus.proposals.clone();
            let unconfirmed_txs = lock.unconfirmed_txs.clone();
            let slot_checkpoints = lock.consensus.slot_checkpoints.clone();
            let leaders_nullifiers = lock.consensus.leaders_nullifiers.clone();
            let leaders_spent_coins = lock.consensus.leaders_spent_coins.clone();
            let response = ConsensusResponse {
                offset,
                proposals,
                unconfirmed_txs,
                slot_checkpoints,
                leaders_nullifiers,
                leaders_spent_coins,
            };
            if let Err(e) = self.channel.send(response).await {
                error!("ProtocolSyncConsensus::handle_receive_request() channel send fail: {}", e);
            };
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolSyncConsensus {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!("ProtocolSyncConsensus::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_request(), executor.clone()).await;
        debug!("ProtocolSyncConsensus::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolSyncConsensus"
    }
}
