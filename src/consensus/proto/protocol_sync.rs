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
use log::{debug, error, info};
use smol::Executor;

use darkfi_sdk::blockchain::SlotCheckpoint;

use crate::{
    consensus::{
        block::{BlockInfo, BlockOrder, BlockResponse},
        state::{SlotCheckpointRequest, SlotCheckpointResponse},
        ValidatorStatePtr,
    },
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};

// Constant defining how many blocks we send during syncing.
const BATCH: u64 = 10;

pub struct ProtocolSync {
    channel: ChannelPtr,
    request_sub: MessageSubscription<BlockOrder>,
    slot_checkpoin_request_sub: MessageSubscription<SlotCheckpointRequest>,
    block_sub: MessageSubscription<BlockInfo>,
    slot_checkpoints_sub: MessageSubscription<SlotCheckpoint>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
    p2p: P2pPtr,
    consensus_mode: bool,
}

impl ProtocolSync {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        p2p: P2pPtr,
        consensus_mode: bool,
    ) -> Result<ProtocolBasePtr> {
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<BlockOrder>().await;
        msg_subsystem.add_dispatch::<SlotCheckpointRequest>().await;
        msg_subsystem.add_dispatch::<BlockInfo>().await;
        msg_subsystem.add_dispatch::<SlotCheckpoint>().await;

        let request_sub = channel.subscribe_msg::<BlockOrder>().await?;
        let slot_checkpoin_request_sub = channel.subscribe_msg::<SlotCheckpointRequest>().await?;
        let block_sub = channel.subscribe_msg::<BlockInfo>().await?;
        let slot_checkpoints_sub = channel.subscribe_msg::<SlotCheckpoint>().await?;

        Ok(Arc::new(Self {
            channel: channel.clone(),
            request_sub,
            slot_checkpoin_request_sub,
            block_sub,
            slot_checkpoints_sub,
            jobsman: ProtocolJobsManager::new("SyncProtocol", channel),
            state,
            p2p,
            consensus_mode,
        }))
    }

    async fn handle_receive_request(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "consensus::protocol_sync::handle_receive_request()",
            "START"
        );
        loop {
            let order = match self.request_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "consensus::protocol_sync::handle_receive_request()",
                        "recv fail: {}",
                        e
                    );
                    continue
                }
            };

            debug!(
                target: "consensus::protocol_sync::handle_receive_request()",
                "received {:?}",
                order
            );

            // Extra validations can be added here
            let key = order.slot;
            let blocks = match self.state.read().await.blockchain.get_blocks_after(key, BATCH) {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "consensus::protocol_sync::handle_receive_request()",
                        "get_blocks_after fail: {}",
                        e
                    );
                    continue
                }
            };
            debug!(
                target: "consensus::protocol_sync::handle_receive_request()",
                "Found {} blocks",
                blocks.len()
            );

            let response = BlockResponse { blocks };
            if let Err(e) = self.channel.send(response).await {
                error!(
                    target: "consensus::protocol_sync::handle_receive_request()",
                    "channel send fail: {}",
                    e
                )
            };
        }
    }

    async fn handle_receive_block(self: Arc<Self>) -> Result<()> {
        debug!(target: "consensus::protocol_sync::handle_receive_block()", "START");
        let exclude_list = vec![self.channel.address()];
        loop {
            let info = match self.block_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "consensus::protocol_sync::handle_receive_block()",
                        "recv fail: {}",
                        e
                    );
                    continue
                }
            };

            // Check if node has finished syncing its blockchain
            if !self.state.read().await.synced {
                debug!(
                    target: "consensus::protocol_sync::handle_receive_block()",
                    "Node still syncing blockchain, skipping..."
                );
                continue
            }

            // Check if node started participating in consensus.
            // Consensus-mode enabled nodes have already performed these steps,
            // during proposal finalization. They still listen to this sub,
            // in case they go out of sync and become a none-consensus node.
            if self.consensus_mode {
                let lock = self.state.read().await;
                let current = lock.consensus.time_keeper.current_slot();
                let participating = lock.consensus.participating;
                if participating.is_some() {
                    let slot = participating.unwrap();
                    if current >= slot {
                        debug!(
                            target: "consensus::protocol_sync::handle_receive_block()",
                            "node runs in consensus mode, skipping..."
                        );
                        continue
                    }
                }
            }

            info!(
                target: "consensus::protocol_sync::handle_receive_block()",
                "Received block: {}",
                info.blockhash()
            );

            debug!(
                target: "consensus::protocol_sync::handle_receive_block()",
                "Processing received block"
            );
            let info_copy = (*info).clone();
            match self.state.write().await.receive_finalized_block(info_copy.clone()).await {
                Ok(v) => {
                    if v {
                        debug!(
                            target: "consensus::protocol_sync::handle_receive_block()",
                            "block processed successfully, broadcasting..."
                        );
                        if let Err(e) =
                            self.p2p.broadcast_with_exclude(info_copy, &exclude_list).await
                        {
                            error!(
                                target: "consensus::protocol_sync::handle_receive_block()",
                                "p2p broadcast fail: {}",
                                e
                            );
                        };
                    }
                }
                Err(e) => {
                    debug!(
                        target: "consensus::protocol_sync::handle_receive_block()",
                        "error processing finalized block: {}",
                        e
                    );
                }
            };
        }
    }

    async fn handle_receive_slot_checkpoint_request(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "consensus::protocol_sync::handle_receive_slot_checkpoint_request()",
            "START"
        );
        loop {
            let request = match self.slot_checkpoin_request_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "consensus::protocol_sync::handle_receive_slot_checkpoint_request()",
                        "recv fail: {}",
                        e
                    );
                    continue
                }
            };

            debug!(
                target: "consensus::protocol_sync::handle_receive_slot_checkpoint_request()",
                "received {:?}",
                request
            );

            // Extra validations can be added here
            let key = request.slot;
            let slot_checkpoints = match self
                .state
                .read()
                .await
                .blockchain
                .get_slot_checkpoints_after(key, BATCH)
            {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "consensus::protocol_sync::handle_receive_slot_checkpoint_request()",
                        "get_slot_checkpoints_after fail: {}",
                        e
                    );
                    continue
                }
            };
            debug!(
                target: "consensus::protocol_sync::handle_receive_slot_checkpoint_request()",
                "Found {} slot checkpoints",
                slot_checkpoints.len()
            );

            let response = SlotCheckpointResponse { slot_checkpoints };
            if let Err(e) = self.channel.send(response).await {
                error!(
                    target: "consensus::protocol_sync::handle_receive_slot_checkpoint_request()",
                    "channel send fail: {}",
                    e
                )
            };
        }
    }

    async fn handle_receive_slot_checkpoint(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "consensus::protocol_sync::handle_receive_slot_checkpoint()",
            "START"
        );
        let exclude_list = vec![self.channel.address()];
        loop {
            let slot_checkpoint = match self.slot_checkpoints_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "consensus::protocol_sync::handle_receive_slot_checkpoint()",
                        "recv fail: {}",
                        e
                    );
                    continue
                }
            };

            // Check if node has finished syncing its blockchain
            if !self.state.read().await.synced {
                debug!(
                    target: "consensus::protocol_sync::handle_receive_slot_checkpoint()",
                    "Node still syncing blockchain, skipping..."
                );
                continue
            }

            // Check if node started participating in consensus.
            // Consensus-mode enabled nodes have already performed these steps,
            // during proposal finalization. They still listen to this sub,
            // in case they go out of sync and become a none-consensus node.
            if self.consensus_mode {
                let lock = self.state.read().await;
                let current = lock.consensus.time_keeper.current_slot();
                let participating = lock.consensus.participating;
                if participating.is_some() {
                    let slot = participating.unwrap();
                    if current >= slot {
                        debug!(
                            target: "consensus::protocol_sync::handle_receive_slot_checkpoint()",
                            "node runs in consensus mode, skipping..."
                        );
                        continue
                    }
                }
            }

            info!(
                target: "consensus::protocol_sync::handle_receive_slot_checkpoint()",
                "Received slot checkpoint: {}",
                slot_checkpoint.slot
            );

            debug!(
                target: "consensus::protocol_sync::handle_receive_slot_checkpoint()",
                "Processing received slot checkpoint"
            );
            let slot_checkpoint_copy = (*slot_checkpoint).clone();
            match self
                .state
                .write()
                .await
                .receive_finalized_slot_checkpoints(slot_checkpoint_copy.clone())
                .await
            {
                Ok(v) => {
                    if v {
                        debug!(
                            target: "consensus::protocol_sync::handle_receive_slot_checkpoint()",
                            "slot checkpoint processed successfully, broadcasting..."
                        );
                        if let Err(e) = self
                            .p2p
                            .broadcast_with_exclude(slot_checkpoint_copy, &exclude_list)
                            .await
                        {
                            error!(
                                target: "consensus::protocol_sync::handle_receive_slot_checkpoint()",
                                "p2p broadcast fail: {}",
                                e
                            );
                        };
                    }
                }
                Err(e) => {
                    debug!(
                        target: "consensus::protocol_sync::handle_receive_slot_checkpoint()",
                        "error processing finalized slot checkpoint: {}",
                        e
                    );
                }
            };
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolSync {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "consensus::protocol_sync::start()", "START");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_request(), executor.clone()).await;
        self.jobsman
            .clone()
            .spawn(self.clone().handle_receive_slot_checkpoint_request(), executor.clone())
            .await;
        self.jobsman.clone().spawn(self.clone().handle_receive_block(), executor.clone()).await;
        self.jobsman
            .clone()
            .spawn(self.clone().handle_receive_slot_checkpoint(), executor.clone())
            .await;
        debug!(target: "consensus::protocol_sync::start()", "END");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolSync"
    }
}
