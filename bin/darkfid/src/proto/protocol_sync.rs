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
    blockchain::{BlockInfo, Header, HeaderHash},
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
pub const BATCH: usize = 20;

/// Structure represening a request to ask a node for their current
/// canonical(finalized) tip block hash, if they are synced. We also
/// include our own tip, so they can verify we follow the same sequence.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct TipRequest {
    /// Canonical(finalized) tip block hash
    pub tip: HeaderHash,
}

impl_p2p_message!(TipRequest, "tiprequest");

/// Structure representing the response to `TipRequest`,
/// containing a boolean flag to indicate if we are synced,
/// and our canonical(finalized) tip block height and hash.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct TipResponse {
    /// Flag indicating the node is synced
    pub synced: bool,
    /// Canonical(finalized) tip block height
    pub height: Option<u32>,
    /// Canonical(finalized) tip block hash
    pub hash: Option<HeaderHash>,
}

impl_p2p_message!(TipResponse, "tipresponse");

/// Structure represening a request to ask a node for up to `BATCH` headers before
/// the provided header height.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct HeaderSyncRequest {
    /// Header height
    pub height: u32,
}

impl_p2p_message!(HeaderSyncRequest, "headersyncrequest");

/// Structure representing the response to `HeaderSyncRequest`,
/// containing up to `BATCH` headers before the requested block height.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct HeaderSyncResponse {
    /// Response headers
    pub headers: Vec<Header>,
}

impl_p2p_message!(HeaderSyncResponse, "headersyncresponse");

/// Structure represening a request to ask a node for up to`BATCH` blocks
/// of provided headers.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct SyncRequest {
    /// Header hashes
    pub headers: Vec<HeaderHash>,
}

impl_p2p_message!(SyncRequest, "syncrequest");

/// Structure representing the response to `SyncRequest`,
/// containing up to `BATCH` blocks after the requested block height.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SyncResponse {
    /// Response blocks
    pub blocks: Vec<BlockInfo>,
}

impl_p2p_message!(SyncResponse, "syncresponse");

/// Structure represening a request to ask a node a fork sequence.
/// If we include a specific fork tip, they have to return its sequence,
/// otherwise they respond with their best fork sequence.
/// We also include our own canonical(finalized) tip, so they can verify
/// we follow the same sequence.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct ForkSyncRequest {
    /// Canonical(finalized) tip block hash
    pub tip: HeaderHash,
    /// Optional fork tip block hash
    pub fork_tip: Option<HeaderHash>,
}

impl_p2p_message!(ForkSyncRequest, "forksyncrequest");

/// Structure representing the response to `ForkSyncRequest`,
/// containing the requested fork sequence.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ForkSyncResponse {
    /// Response fork proposals
    pub proposals: Vec<Proposal>,
}

impl_p2p_message!(ForkSyncResponse, "forksyncresponse");

pub struct ProtocolSync {
    tip_sub: MessageSubscription<TipRequest>,
    header_sub: MessageSubscription<HeaderSyncRequest>,
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
        msg_subsystem.add_dispatch::<TipRequest>().await;
        msg_subsystem.add_dispatch::<TipResponse>().await;
        msg_subsystem.add_dispatch::<HeaderSyncRequest>().await;
        msg_subsystem.add_dispatch::<HeaderSyncResponse>().await;
        msg_subsystem.add_dispatch::<SyncRequest>().await;
        msg_subsystem.add_dispatch::<SyncResponse>().await;
        msg_subsystem.add_dispatch::<ForkSyncRequest>().await;
        msg_subsystem.add_dispatch::<ForkSyncResponse>().await;

        let tip_sub = channel.subscribe_msg::<TipRequest>().await?;
        let header_sub = channel.subscribe_msg::<HeaderSyncRequest>().await?;
        let request_sub = channel.subscribe_msg::<SyncRequest>().await?;
        let fork_request_sub = channel.subscribe_msg::<ForkSyncRequest>().await?;

        Ok(Arc::new(Self {
            tip_sub,
            header_sub,
            request_sub,
            fork_request_sub,
            jobsman: ProtocolJobsManager::new("SyncProtocol", channel.clone()),
            validator,
            channel,
        }))
    }

    async fn handle_receive_tip_request(self: Arc<Self>) -> Result<()> {
        debug!(target: "darkfid::proto::protocol_sync::handle_receive_tip_request", "START");
        loop {
            let request = match self.tip_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "darkfid::proto::protocol_sync::handle_receive_tip_request",
                        "recv fail: {e}"
                    );
                    continue
                }
            };

            // Check if node has finished syncing its blockchain
            let response = if !*self.validator.synced.read().await {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_tip_request",
                    "Node still syncing blockchain, skipping..."
                );
                TipResponse { synced: false, height: None, hash: None }
            } else {
                // Check we follow the same sequence
                match self.validator.blockchain.blocks.contains(&request.tip) {
                    Ok(contains) => {
                        if !contains {
                            debug!(
                                target: "darkfid::proto::protocol_sync::handle_receive_tip_request",
                                "Node doesn't follow request sequence"
                            );
                            continue
                        }
                    }
                    Err(e) => {
                        error!(
                            target: "darkfid::proto::protocol_sync::handle_receive_tip_request",
                            "block_store.contains fail: {e}"
                        );
                        continue
                    }
                }

                // Grab our current tip and return it
                let tip = match self.validator.blockchain.last() {
                    Ok(v) => v,
                    Err(e) => {
                        error!(
                            target: "darkfid::proto::protocol_sync::handle_receive_tip_request",
                            "blockchain.last fail: {e}"
                        );
                        continue
                    }
                };

                TipResponse { synced: true, height: Some(tip.0), hash: Some(tip.1) }
            };

            if let Err(e) = self.channel.send(&response).await {
                error!(
                    target: "darkfid::proto::protocol_sync::handle_receive_tip_request",
                    "Channel send fail: {e}"
                )
            };
        }
    }

    async fn handle_receive_header_request(self: Arc<Self>) -> Result<()> {
        debug!(target: "darkfid::proto::protocol_sync::handle_receive_header_request", "START");
        loop {
            let request = match self.header_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "darkfid::proto::protocol_sync::handle_receive_header_request",
                        "recv fail: {}",
                        e
                    );
                    continue
                }
            };

            // Check if node has finished syncing its blockchain
            if !*self.validator.synced.read().await {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_header_request",
                    "Node still syncing blockchain, skipping..."
                );
                continue
            }

            let headers = match self.validator.blockchain.get_headers_before(request.height, BATCH)
            {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "darkfid::proto::protocol_sync::handle_receive_header_request",
                        "get_headers_before fail: {}",
                        e
                    );
                    continue
                }
            };

            let response = HeaderSyncResponse { headers };
            if let Err(e) = self.channel.send(&response).await {
                error!(
                    target: "darkfid::proto::protocol_sync::handle_receive_header_request",
                    "channel send fail: {}",
                    e
                )
            };
        }
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

            // Check if request exists the configured limit
            if request.headers.len() > BATCH {
                debug!(
                    target: "darkfid::proto::protocol_sync::handle_receive_request",
                    "Node requested more blocks than allowed."
                );
                continue
            }

            let blocks = match self.validator.blockchain.get_blocks_by_hash(&request.headers) {
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

            debug!(target: "darkfid::proto::protocol_sync::handle_receive_request", "Received request: {request:?}");

            // If a fork tip is provided, grab its fork proposals sequence.
            // Otherwise, grab best fork proposals sequence.
            let proposals = match request.fork_tip {
                Some(fork_tip) => {
                    self.validator
                        .consensus
                        .get_fork_proposals(request.tip, fork_tip, BATCH as u32)
                        .await
                }
                None => {
                    self.validator
                        .consensus
                        .get_best_fork_proposals(request.tip, BATCH as u32)
                        .await
                }
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
            debug!(target: "darkfid::proto::protocol_sync::handle_receive_request", "Response: {response:?}");
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
        self.jobsman
            .clone()
            .spawn(self.clone().handle_receive_tip_request(), executor.clone())
            .await;
        self.jobsman
            .clone()
            .spawn(self.clone().handle_receive_header_request(), executor.clone())
            .await;
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
