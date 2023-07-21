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
use url::Url;

use crate::{
    blockchain::BlockInfo,
    impl_p2p_message,
    net::{
        ChannelPtr, Message, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    validator::ValidatorPtr,
    Result,
};

pub struct ProtocolBlock {
    block_sub: MessageSubscription<BlockInfo>,
    jobsman: ProtocolJobsManagerPtr,
    validator: ValidatorPtr,
    p2p: P2pPtr,
    channel_address: Url,
}

impl_p2p_message!(BlockInfo, "block");

impl ProtocolBlock {
    pub async fn init(
        channel: ChannelPtr,
        validator: ValidatorPtr,
        p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        debug!(
            target: "validator::protocol_block::init",
            "Adding ProtocolTx to the protocol registry"
        );
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<BlockInfo>().await;

        let block_sub = channel.subscribe_msg::<BlockInfo>().await?;

        Ok(Arc::new(Self {
            block_sub,
            jobsman: ProtocolJobsManager::new("BlockProtocol", channel.clone()),
            validator,
            p2p,
            channel_address: channel.address().clone(),
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

            let block_copy = (*block).clone();

            match self.validator.write().await.append_block(&block_copy).await {
                Ok(()) => self.p2p.broadcast_with_exclude(&block_copy, &exclude_list).await,
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
