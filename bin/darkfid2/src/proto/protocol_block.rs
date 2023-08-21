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
use log::debug;
use smol::Executor;
use tinyjson::JsonValue;
use url::Url;

use darkfi::{
    blockchain::BlockInfo,
    impl_p2p_message,
    net::{
        ChannelPtr, Message, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    rpc::jsonrpc::JsonSubscriber,
    util::encoding::base64,
    validator::ValidatorPtr,
    Result,
};
use darkfi_serial::{serialize, SerialDecodable, SerialEncodable};

/// Auxiliary [`BlockInfo`] wrapper structure used for messaging.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct BlockInfoMessage(BlockInfo);

impl From<&BlockInfo> for BlockInfoMessage {
    fn from(block: &BlockInfo) -> Self {
        BlockInfoMessage(block.clone())
    }
}

impl_p2p_message!(BlockInfoMessage, "block");

pub struct ProtocolBlock {
    block_sub: MessageSubscription<BlockInfoMessage>,
    jobsman: ProtocolJobsManagerPtr,
    validator: ValidatorPtr,
    p2p: P2pPtr,
    channel_address: Url,
    subscriber: JsonSubscriber,
}

impl ProtocolBlock {
    pub async fn init(
        channel: ChannelPtr,
        validator: ValidatorPtr,
        p2p: P2pPtr,
        subscriber: JsonSubscriber,
    ) -> Result<ProtocolBasePtr> {
        debug!(
            target: "validator::protocol_block::init",
            "Adding ProtocolBlock to the protocol registry"
        );
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<BlockInfoMessage>().await;

        let block_sub = channel.subscribe_msg::<BlockInfoMessage>().await?;

        Ok(Arc::new(Self {
            block_sub,
            jobsman: ProtocolJobsManager::new("BlockProtocol", channel.clone()),
            validator,
            p2p,
            channel_address: channel.address().clone(),
            subscriber,
        }))
    }

    async fn handle_receive_block(self: Arc<Self>) -> Result<()> {
        debug!(target: "consensus::protocol_block::handle_receive_block", "START");
        let exclude_list = vec![self.channel_address.clone()];
        loop {
            let block = match self.block_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "validator::protocol_block::handle_receive_block",
                        "recv fail: {}",
                        e
                    );
                    continue
                }
            };

            // Check if node has finished syncing its blockchain
            if !self.validator.read().await.synced {
                debug!(
                    target: "validator::protocol_block::handle_receive_block",
                    "Node still syncing blockchain, skipping..."
                );
                continue
            }

            // Check if node started participating in consensus.
            // Consensus-mode enabled nodes have already performed these steps,
            // during proposal finalization. They still listen to this sub,
            // in case they go out of sync and become a none-consensus node.
            if self.validator.read().await.consensus.participating {
                debug!(
                    target: "validator::protocol_block::handle_receive_block",
                    "Node is participating in consensus, skipping..."
                );
                continue
            }

            let block_copy = (*block).clone();

            match self.validator.write().await.append_block(&block_copy.0).await {
                Ok(()) => {
                    self.p2p.broadcast_with_exclude(&block_copy, &exclude_list).await;
                    let encoded_block = JsonValue::String(base64::encode(&serialize(&block_copy)));
                    self.subscriber.notify(vec![encoded_block]).await;
                }
                Err(e) => {
                    debug!(
                        target: "validator::protocol_block::handle_receive_block",
                        "append_block fail: {}",
                        e
                    );
                }
            };
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolBlock {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "validator::protocol_block::start", "START");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_block(), executor.clone()).await;
        debug!(target: "validator::protocol_block::start", "END");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolBlock"
    }
}
