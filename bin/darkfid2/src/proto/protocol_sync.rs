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
    validator::ValidatorPtr,
    Result,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

// Constant defining how many blocks we send during syncing.
const BATCH: u64 = 10;

/// Auxiliary structure used for blockchain syncing.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct SyncRequest {
    /// Slot UID
    pub slot: u64,
    /// Block headerhash of that slot
    pub block: blake3::Hash,
}

impl_p2p_message!(SyncRequest, "syncrequest");

/// Auxiliary structure used for blockchain syncing.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SyncResponse {
    /// Response blocks
    pub blocks: Vec<BlockInfo>,
}

impl_p2p_message!(SyncResponse, "syncresponse");

pub struct ProtocolSync {
    request_sub: MessageSubscription<SyncRequest>,
    jobsman: ProtocolJobsManagerPtr,
    validator: ValidatorPtr,
    channel: ChannelPtr,
}

impl ProtocolSync {
    pub async fn init(channel: ChannelPtr, validator: ValidatorPtr) -> Result<ProtocolBasePtr> {
        debug!(
            target: "validator::protocol_sync::init",
            "Adding ProtocolSync to the protocol registry"
        );
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<SyncRequest>().await;

        let request_sub = channel.subscribe_msg::<SyncRequest>().await?;

        Ok(Arc::new(Self {
            request_sub,
            jobsman: ProtocolJobsManager::new("SyncProtocol", channel.clone()),
            validator,
            channel,
        }))
    }

    async fn handle_receive_request(self: Arc<Self>) -> Result<()> {
        debug!(target: "validator::protocol_sync::handle_receive_request", "START");
        loop {
            let request = match self.request_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "validator::protocol_sync::handle_receive_request",
                        "recv fail: {}",
                        e
                    );
                    continue
                }
            };

            // Check if node has finished syncing its blockchain
            if !*self.validator.synced.read().await {
                debug!(
                    target: "validator::protocol_sync::handle_receive_request",
                    "Node still syncing blockchain, skipping..."
                );
                continue
            }

            let key = request.slot;
            let blocks = match self.validator.blockchain.get_blocks_after(key, BATCH) {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "validator::protocol_sync::handle_receive_request",
                        "get_blocks_after fail: {}",
                        e
                    );
                    continue
                }
            };

            let response = SyncResponse { blocks };
            if let Err(e) = self.channel.send(&response).await {
                error!(
                    target: "validator::protocol_sync::handle_receive_request",
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
        debug!(target: "validator::protocol_sync::start", "START");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_request(), executor.clone()).await;
        debug!(target: "validator::protocol_sync::start", "END");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolSync"
    }
}
