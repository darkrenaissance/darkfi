use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use log::debug;

use crate::{
    consensus2::{
        block::{ForkOrder, ForkResponse},
        state::ValidatorStatePtr,
    },
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};

pub struct ProtocolSyncForks {
    channel: ChannelPtr,
    order_sub: MessageSubscription<ForkOrder>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
}

impl ProtocolSyncForks {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        _p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<ForkOrder>().await;

        let order_sub = channel.subscribe_msg::<ForkOrder>().await?;

        Ok(Arc::new(Self {
            channel: channel.clone(),
            order_sub,
            jobsman: ProtocolJobsManager::new("SyncForkProtocol", channel),
            state,
        }))
    }

    async fn handle_receive_order(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolSyncForks::handle_receive_order() [START]");
        loop {
            let order = self.order_sub.receive().await?;

            debug!("ProtocolSyncForks::handle_receive_order() received {:?}", order);

            // Extra validations can be added here.
            let proposals = self.state.read().await.consensus.proposals.clone();
            let response = ForkResponse { proposals };
            self.channel.send(response).await?;
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolSyncForks {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!("ProtocolSyncForks::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_order(), executor.clone()).await;
        debug!("ProtocolSyncForks::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolSyncForks"
    }
}
