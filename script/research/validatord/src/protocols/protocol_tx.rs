use async_executor::Executor;
use async_trait::async_trait;

use darkfi::{
    consensus::{state::StatePtr, tx::Tx},
    net::{
        ChannelPtr, MessageSubscription, ProtocolBase, ProtocolBasePtr, ProtocolJobsManager,
        ProtocolJobsManagerPtr,
    },
    Result,
};
use log::debug;
use std::sync::Arc;

pub struct ProtocolTx {
    tx_sub: MessageSubscription<Tx>,
    jobsman: ProtocolJobsManagerPtr,
    state: StatePtr,
}

impl ProtocolTx {
    pub async fn init(channel: ChannelPtr, state: StatePtr) -> ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Tx>().await;

        let tx_sub = channel.subscribe_msg::<Tx>().await.expect("Missing Tx dispatcher!");

        Arc::new(Self { tx_sub, jobsman: ProtocolJobsManager::new("TxProtocol", channel), state })
    }

    async fn handle_receive_tx(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolTx::handle_receive_tx() [START]");
        loop {
            let tx = self.tx_sub.receive().await?;

            debug!(
                target: "ircd",
                "ProtocolTx::handle_receive_tx() received {:?}",
                tx
            );
            let tx_copy = (*tx).clone();
            self.state.write().unwrap().append_tx(tx_copy.clone());
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolTx {
    /// Starts ping-pong keep-alive messages exchange. Runs ping-pong in the
    /// protocol task manager, then queues the reply. Sends out a ping and
    /// waits for pong reply. Waits for ping and replies with a pong.
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "ircd", "ProtocolTx::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_tx(), executor.clone()).await;
        debug!(target: "ircd", "ProtocolTx::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolTx"
    }
}
