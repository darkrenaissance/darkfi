use async_executor::Executor;
use async_trait::async_trait;

use darkfi::{
    consensus::{
        blockchain::{ForkOrder, ForkResponse},
        state::ValidatorStatePtr,
    },
    net::{
        ChannelPtr, MessageSubscription, ProtocolBase, ProtocolBasePtr, ProtocolJobsManager,
        ProtocolJobsManagerPtr,
    },
    Result,
};
use log::debug;
use std::sync::Arc;

pub struct ProtocolSyncForks {
    channel: ChannelPtr,
    order_sub: MessageSubscription<ForkOrder>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
}

impl ProtocolSyncForks {
    pub async fn init(channel: ChannelPtr, state: ValidatorStatePtr) -> ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<ForkOrder>().await;

        let order_sub =
            channel.subscribe_msg::<ForkOrder>().await.expect("Missing ForkOrder dispatcher!");

        Arc::new(Self {
            channel: channel.clone(),
            order_sub,
            jobsman: ProtocolJobsManager::new("SyncForkProtocol", channel),
            state,
        })
    }

    async fn handle_receive_order(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolSyncForks::handle_receive_tx() [START]");
        loop {
            let order = self.order_sub.receive().await?;

            debug!(
                target: "ircd",
                "ProtocolSyncForks::handle_receive_order() received {:?}",
                order
            );

            // Extra validations can be added here.
            let proposals = self.state.read().unwrap().consensus.proposals.clone();
            let response = ForkResponse { proposals };
            self.channel.send(response).await?;
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolSyncForks {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "ircd", "ProtocolSyncForks::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_order(), executor.clone()).await;
        debug!(target: "ircd", "ProtocolSyncForks::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolSyncForks"
    }
}
