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
use log::{debug, error};
use smol::Executor;

use darkfi::{
    blockchain::BlockInfo,
    impl_p2p_message,
    net::{
        ChannelPtr, Message, MessageSubscription, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    validator::{consensus::Proposal, ValidatorPtr},
    Result,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

// Constant defining how many blocks we send during syncing.
const BATCH: u64 = 10;

/// Auxiliary structure used for blockchain syncing.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct SyncRequest {
    /// Block height
    pub height: u64,
}

impl_p2p_message!(SyncRequest, "syncrequest");

/// Auxiliary structure used for blockchain syncing.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SyncResponse {
    /// Response blocks
    pub blocks: Vec<BlockInfo>,
}

impl_p2p_message!(SyncResponse, "syncresponse");

/// Auxiliary structure used for fork chain syncing.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct ForkSyncRequest {
    /// Canonical(finalized) tip block hash
    pub tip: blake3::Hash,
    /// Optional fork tip block hash
    pub fork_tip: Option<blake3::Hash>,
}

impl_p2p_message!(ForkSyncRequest, "forksyncrequest");

/// Auxiliary structure used for fork chain syncing.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ForkSyncResponse {
    /// Response fork proposals
    pub proposals: Vec<Proposal>,
}

impl_p2p_message!(ForkSyncResponse, "forksyncresponse");

pub struct ProtocolSync {
    request_sub: MessageSubscription<SyncRequest>,
    fork_request_sub: MessageSubscription<ForkSyncRequest>,
    jobsman: ProtocolJobsManagerPtr,
    validator: ValidatorPtr,
    channel: ChannelPtr,
}

impl ProtocolSync {
    pub async fn init(channel: ChannelPtr, validator: ValidatorPtr) -> Result<ProtocolBasePtr> {
        debug!(
            target: "darkfid::proto::protocol_sync::init",
            "Adding ProtocolSync to the protocol registry"
        );
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<SyncRequest>().await;
        msg_subsystem.add_dispatch::<ForkSyncRequest>().await;

        let request_sub = channel.subscribe_msg::<SyncRequest>().await?;
        let fork_request_sub = channel.subscribe_msg::<ForkSyncRequest>().await?;

        Ok(Arc::new(Self {
            request_sub,
            fork_request_sub,
            jobsman: ProtocolJobsManager::new("SyncProtocol", channel.clone()),
            validator,
            channel,
        }))
    }

    async fn handle_receive_request(self: Arc<Self>) -> Result<()> {
        debug!(target: "darkfid::proto::protocol_sync::handle_receive_request", "START");
        loop {
            let request = match self.request_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "darkfid::proto::protocol_sync::handle_receive_request",
                        "recv fail: {}",
                        e
                    );
                    continue
                }
            };

            // Check if node has finished syncing its blockchain
            if !*self.validator.synced.read().await {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_request",
                    "Node still syncing blockchain, skipping..."
                );
                continue
            }

            let blocks = match self.validator.blockchain.get_blocks_after(request.height, BATCH) {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "darkfid::proto::protocol_sync::handle_receive_request",
                        "get_blocks_after fail: {}",
                        e
                    );
                    continue
                }
            };

            let response = SyncResponse { blocks };
            if let Err(e) = self.channel.send(&response).await {
                error!(
                    target: "darkfid::proto::protocol_sync::handle_receive_request",
                    "channel send fail: {}",
                    e
                )
            };
        }
    }

    async fn handle_receive_fork_request(self: Arc<Self>) -> Result<()> {
        debug!(target: "darkfid::proto::protocol_sync::handle_receive_fork_request", "START");
        loop {
            let request = match self.fork_request_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "darkfid::proto::protocol_sync::handle_receive_fork_request",
                        "recv fail: {}",
                        e
                    );
                    continue
                }
            };

            // Check if node has finished syncing its blockchain
            if !*self.validator.synced.read().await {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_fork_request",
                    "Node still syncing blockchain, skipping..."
                );
                continue
            }

            // If a fork tip is provided, grab its fork proposals sequence.
            // Otherwise, grab best fork proposals sequence.
            let proposals = match request.fork_tip {
                Some(fork_tip) => {
                    self.validator.consensus.get_fork_proposals(request.tip, fork_tip).await
                }
                None => self.validator.consensus.get_best_fork_proposals(request.tip).await,
            };
            let proposals = match proposals {
                Ok(p) => p,
                Err(e) => {
                    debug!(
                        target: "darkfid::proto::protocol_sync::handle_receive_request",
                        "Getting fork proposals failed: {}",
                        e
                    );
                    continue
                }
            };

            let response = ForkSyncResponse { proposals };
            if let Err(e) = self.channel.send(&response).await {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_fork_request",
                    "channel send fail: {}",
                    e
                )
            };
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolSync {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "darkfid::proto::protocol_sync::start", "START");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_request(), executor.clone()).await;
        self.jobsman
            .clone()
            .spawn(self.clone().handle_receive_fork_request(), executor.clone())
            .await;
        debug!(target: "darkfid::proto::protocol_sync::start", "END");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolSync"
    }
}
