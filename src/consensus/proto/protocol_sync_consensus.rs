use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use log::{debug, error};

use crate::{
    consensus::{
        state::{ConsensusRequest, ConsensusResponse},
        ValidatorStatePtr,
    },
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};

pub struct ProtocolSyncConsensus {
    channel: ChannelPtr,
    request_sub: MessageSubscription<ConsensusRequest>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
}

impl ProtocolSyncConsensus {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        _p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<ConsensusRequest>().await;

        let request_sub = channel.subscribe_msg::<ConsensusRequest>().await?;

        Ok(Arc::new(Self {
            channel: channel.clone(),
            request_sub,
            jobsman: ProtocolJobsManager::new("SyncConsensusProtocol", channel),
            state,
        }))
    }

    async fn handle_receive_request(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolSyncConsensus::handle_receive_request() [START]");
        loop {
            let order = match self.request_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!("ProtocolSyncConsensus::handle_receive_request() recv fail: {}", e);
                    continue
                }
            };

            debug!("ProtocolSyncConsensuss::handle_receive_request() received {:?}", order);

            // Extra validations can be added here.
            let consensus = self.state.read().await.consensus.clone();
            let response = ConsensusResponse { consensus };
            if let Err(e) = self.channel.send(response).await {
                error!("ProtocolSyncConsensus::handle_receive_request() channel send fail: {}", e);
            };
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolSyncConsensus {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!("ProtocolSyncConsensus::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_request(), executor.clone()).await;
        debug!("ProtocolSyncConsensus::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolSyncConsensus"
    }
}
