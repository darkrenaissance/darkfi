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

use async_std::sync::Arc;
use async_trait::async_trait;
use log::{debug, error};
use smol::Executor;

use crate::{
    consensus::{
        state::{
            ConsensusRequest, ConsensusResponse, ConsensusSlotCheckpointsRequest,
            ConsensusSlotCheckpointsResponse,
        },
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
    slot_checkpoints_request_sub: MessageSubscription<ConsensusSlotCheckpointsRequest>,
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
        msg_subsystem.add_dispatch::<ConsensusSlotCheckpointsRequest>().await;

        let request_sub = channel.subscribe_msg::<ConsensusRequest>().await?;
        let slot_checkpoints_request_sub =
            channel.subscribe_msg::<ConsensusSlotCheckpointsRequest>().await?;

        Ok(Arc::new(Self {
            channel: channel.clone(),
            request_sub,
            slot_checkpoints_request_sub,
            jobsman: ProtocolJobsManager::new("SyncConsensusProtocol", channel),
            state,
        }))
    }

    async fn handle_receive_request(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "consensus::protocol_sync_consensus::handle_receive_request()",
            "START"
        );
        loop {
            let req = match self.request_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "consensus::protocol_sync_consensus::handle_receive_request()",
                        "recv fail: {}",
                        e
                    );
                    continue
                }
            };

            debug!(
                target: "consensus::protocol_sync_consensus::handle_receive_request()",
                "received {:?}",
                req
            );

            // Extra validations can be added here.
            let lock = self.state.read().await;
            let bootstrap_slot = lock.consensus.bootstrap_slot;
            let current_slot = lock.consensus.current_slot();
            let mut forks = vec![];
            for fork in &lock.consensus.forks {
                forks.push(fork.clone().into());
            }
            let pending_txs = match lock.blockchain.get_pending_txs() {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "consensus::protocol_sync_consensus::handle_receive_request()",
                        "Failed querying pending txs store: {}",
                        e
                    );
                    vec![]
                }
            };
            let slot_checkpoints = lock.consensus.slot_checkpoints.clone();
            let mut f_history = vec![];
            for f in &lock.consensus.f_history {
                let f_str = format!("{:}", f);
                f_history.push(f_str);
            }
            let mut err_history = vec![];
            for err in &lock.consensus.err_history {
                let err_str = format!("{:}", err);
                err_history.push(err_str);
            }
            let nullifiers = lock.consensus.nullifiers.clone();
            let response = ConsensusResponse {
                bootstrap_slot,
                current_slot,
                forks,
                pending_txs,
                slot_checkpoints,
                f_history,
                err_history,
                nullifiers,
            };
            if let Err(e) = self.channel.send(response).await {
                error!(
                    target: "consensus::protocol_sync_consensus::handle_receive_request()",
                    "channel send fail: {}",
                    e
                );
            };
        }
    }

    async fn handle_receive_slot_checkpoints_request(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "consensus::protocol_sync_consensus::handle_receive_slot_checkpoints_request()",
            "START"
        );
        loop {
            let req = match self.slot_checkpoints_request_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "consensus::protocol_sync_consensus::handle_receive_slot_checkpoints_request()",
                        "recv fail: {}",
                        e
                    );
                    continue
                }
            };

            debug!(
                target: "consensus::protocol_sync_consensus::handle_receive_slot_checkpoints_request()",
                "received {:?}",
                req
            );

            // Extra validations can be added here.
            let lock = self.state.read().await;
            let bootstrap_slot = lock.consensus.bootstrap_slot;
            let proposing = lock.consensus.proposing;
            let is_empty = lock.consensus.slot_checkpoints_is_empty();
            let response = ConsensusSlotCheckpointsResponse { bootstrap_slot, proposing, is_empty };
            if let Err(e) = self.channel.send(response).await {
                error!(
                    target: "consensus::protocol_sync_consensus::handle_receive_slot_checkpoints_request()",
                    "channel send fail: {}",
                    e
                );
            };
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolSyncConsensus {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(
            target: "consensus::protocol_sync_consensus::start()",
            "START"
        );
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_request(), executor.clone()).await;
        self.jobsman
            .clone()
            .spawn(self.clone().handle_receive_slot_checkpoints_request(), executor.clone())
            .await;
        debug!(
            target: "consensus::protocol_sync_consensus::start()",
            "END"
        );
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolSyncConsensus"
    }
}
