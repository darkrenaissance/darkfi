use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use log::debug;

use darkfi::{
    consensus2::{state::ValidatorStatePtr, Tx},
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};

pub struct ProtocolTx {
    tx_sub: MessageSubscription<Tx>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
    p2p: P2pPtr,
}

impl ProtocolTx {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<Tx>().await;

        let tx_sub = channel.subscribe_msg::<Tx>().await?;

        Ok(Arc::new(Self {
            tx_sub,
            jobsman: ProtocolJobsManager::new("TxProtocol", channel),
            state,
            p2p,
        }))
    }

    async fn handle_receive_tx(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolTx::handle_receive_tx() [START]");
        loop {
            let tx = self.tx_sub.receive().await?;

            debug!("ProtocolTx::handle_receive_tx() recv: {:?}", tx);

            let tx_copy = (*tx).clone();
            if self.state.write().unwrap().append_tx(tx_copy.clone()) {
                self.p2p.broadcast(tx_copy).await?;
            }
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolTx {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!("ProtocolTx::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_tx(), executor.clone()).await;
        debug!("ProtocolTx::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolTx"
    }
}
