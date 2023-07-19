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

use darkfi::{
    consensus::ValidatorStatePtr,
    impl_p2p_message,
    net::{
        ChannelPtr, Message, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    tx::Transaction,
    Result,
};

pub struct ProtocolTx {
    tx_sub: MessageSubscription<Transaction>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
    p2p: P2pPtr,
    channel_address: Url,
}

impl_p2p_message!(Transaction, "tx");

impl ProtocolTx {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        debug!(
            target: "darkfid::protocol_tx::init",
            "Adding ProtocolTx to the protocol registry"
        );
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<Transaction>().await;

        let tx_sub = channel.subscribe_msg::<Transaction>().await?;

        Ok(Arc::new(Self {
            tx_sub,
            jobsman: ProtocolJobsManager::new("TxProtocol", channel.clone()),
            state,
            p2p,
            channel_address: channel.address().clone(),
        }))
    }

    async fn handle_receive_tx(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "darkfid::protocol_tx::handle_receive_tx",
            "START"
        );
        let exclude_list = vec![self.channel_address.clone()];
        loop {
            let tx = match self.tx_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    debug!(
                        target: "darkfid::protocol_tx::handle_receive_tx",
                        "recv fail: {}",
                        e
                    );
                    continue
                }
            };

            // Check if node has finished syncing its blockchain
            if !self.state.read().await.synced {
                debug!(
                    target: "darkfid::protocol_tx::handle_receive_tx",
                    "Node still syncing blockchain, skipping..."
                );
                continue
            }

            let tx_copy = (*tx).clone();

            // Nodes use unconfirmed_txs vector as seen_txs pool.
            if self.state.write().await.append_tx(tx_copy.clone()).await {
                self.p2p.broadcast_with_exclude(&tx_copy, &exclude_list).await;
            }
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolTx {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "darkfid::protocol_tx::start", "START");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_tx(), executor.clone()).await;
        debug!(target: "darkfid::protocol_tx::start", "END");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolTx"
    }
}
