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
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    rpc::jsonrpc::JsonSubscriber,
    tx::Transaction,
    util::encoding::base64,
    validator::ValidatorPtr,
    Result,
};
use darkfi_serial::serialize_async;

pub struct ProtocolTx {
    tx_sub: MessageSubscription<Transaction>,
    jobsman: ProtocolJobsManagerPtr,
    validator: ValidatorPtr,
    p2p: P2pPtr,
    channel_address: Url,
    subscriber: JsonSubscriber,
}

impl ProtocolTx {
    pub async fn init(
        channel: ChannelPtr,
        validator: ValidatorPtr,
        p2p: P2pPtr,
        subscriber: JsonSubscriber,
    ) -> Result<ProtocolBasePtr> {
        debug!(
            target: "validator::protocol_tx::init",
            "Adding ProtocolTx to the protocol registry"
        );
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<Transaction>().await;

        let tx_sub = channel.subscribe_msg::<Transaction>().await?;

        Ok(Arc::new(Self {
            tx_sub,
            jobsman: ProtocolJobsManager::new("TxProtocol", channel.clone()),
            validator,
            p2p,
            channel_address: channel.address().clone(),
            subscriber,
        }))
    }

    async fn handle_receive_tx(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "validator::protocol_tx::handle_receive_tx",
            "START"
        );
        let exclude_list = vec![self.channel_address.clone()];
        loop {
            let tx = match self.tx_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "validator::protocol_tx::handle_receive_tx",
                        "recv fail: {}",
                        e
                    );
                    continue
                }
            };

            // Check if node has finished syncing its blockchain
            if !*self.validator.synced.read().await {
                debug!(
                    target: "validator::protocol_tx::handle_receive_tx",
                    "Node still syncing blockchain, skipping..."
                );
                continue
            }

            let tx_copy = (*tx).clone();

            // Nodes use unconfirmed_txs vector as seen_txs pool.
            match self.validator.append_tx(&tx_copy).await {
                Ok(()) => {
                    self.p2p.broadcast_with_exclude(&tx_copy, &exclude_list).await;
                    let encoded_tx =
                        JsonValue::String(base64::encode(&serialize_async(&tx_copy).await));
                    self.subscriber.notify(vec![encoded_tx].into()).await;
                }
                Err(e) => {
                    debug!(
                        target: "validator::protocol_tx::handle_receive_tx",
                        "append_tx fail: {}",
                        e
                    );
                }
            }
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolTx {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "validator::protocol_tx::start", "START");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_tx(), executor.clone()).await;
        debug!(target: "validator::protocol_tx::start", "END");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolTx"
    }
}
