use async_executor::Executor;
use async_trait::async_trait;

use darkfi::{
    consensus::state::{ConsensusRequest, ConsensusResponse, ValidatorStatePtr},
    net::{
        ChannelPtr, MessageSubscription, ProtocolBase, ProtocolBasePtr, ProtocolJobsManager,
        ProtocolJobsManagerPtr,
    },
    Result,
};
use log::debug;
use std::sync::Arc;

pub struct ProtocolSyncConsensus {
    channel: ChannelPtr,
    request_sub: MessageSubscription<ConsensusRequest>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
}

impl ProtocolSyncConsensus {
    pub async fn init(channel: ChannelPtr, state: ValidatorStatePtr) -> ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<ConsensusRequest>().await;

        let request_sub = channel
            .subscribe_msg::<ConsensusRequest>()
            .await
            .expect("Missing ConsensusRequest dispatcher!");

        Arc::new(Self {
            channel: channel.clone(),
            request_sub,
            jobsman: ProtocolJobsManager::new("SyncConsensusProtocol", channel),
            state,
        })
    }

    async fn handle_receive_request(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolSyncConsensus::handle_receive_request() [START]");
        loop {
            let order = self.request_sub.receive().await?;

            debug!(
                target: "ircd",
                "ProtocolSyncConsensus::handle_receive_request() received {:?}",
                order
            );

            // Extra validations can be added here.
            let consensus = self.state.read().unwrap().consensus.clone();
            let response = ConsensusResponse { consensus };
            self.channel.send(response).await?;
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolSyncConsensus {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "ircd", "ProtocolSyncConsensus::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_request(), executor.clone()).await;
        debug!(target: "ircd", "ProtocolSyncConsensus::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolSyncConsensus"
    }
}
