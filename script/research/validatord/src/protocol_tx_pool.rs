use async_executor::Executor;
use async_trait::async_trait;

use darkfi::{net, Result};
use log::debug;
use std::sync::Arc;

use crate::tx_pool::{SeenTxHashesPtr, Tx};

pub struct ProtocolTxPool {
    notify_queue_sender: async_channel::Sender<Arc<Tx>>,
    tx_pool_sub: net::MessageSubscription<Tx>,
    jobsman: net::ProtocolJobsManagerPtr,
    seen_tx_hashes: SeenTxHashesPtr,
    p2p: net::P2pPtr,
}

impl ProtocolTxPool {
    pub async fn init(
        channel: net::ChannelPtr,
        notify_queue_sender: async_channel::Sender<Arc<Tx>>,
        seen_tx_hashes: SeenTxHashesPtr,
        p2p: net::P2pPtr,
    ) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Tx>().await;

        let tx_sub = channel.subscribe_msg::<Tx>().await.expect("Missing Tx dispatcher!");

        Arc::new(Self {
            notify_queue_sender,
            tx_pool_sub: tx_sub,
            jobsman: net::ProtocolJobsManager::new("TxPoolProtocol", channel),
            seen_tx_hashes,
            p2p,
        })
    }

    async fn handle_receive_tx(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolTxPool::handle_receive_tx() [START]");
        loop {
            let tx = self.tx_pool_sub.receive().await?;

            debug!(
                target: "ircd",
                "ProtocolTxPool::handle_receive_tx() received {:?}",
                tx
            );

            // Do we already have this tx?
            if self.seen_tx_hashes.is_seen(tx.hash).await {
                continue
            }

            self.seen_tx_hashes.add_seen(tx.hash).await;

            // If not then broadcast to everybody else
            let tx_copy = (*tx).clone();
            self.p2p.broadcast(tx_copy).await?;

            self.notify_queue_sender.send(tx).await.expect("notify_queue_sender send failed!");
        }
    }
}

#[async_trait]
impl net::ProtocolBase for ProtocolTxPool {
    /// Starts ping-pong keep-alive messages exchange. Runs ping-pong in the
    /// protocol task manager, then queues the reply. Sends out a ping and
    /// waits for pong reply. Waits for ping and replies with a pong.
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "ircd", "ProtocolTxPool::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_tx(), executor.clone()).await;
        debug!(target: "ircd", "ProtocolTxPool::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolTxPool"
    }
}
