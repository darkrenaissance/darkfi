use async_std::sync::Arc;

use async_executor::Executor;
use async_trait::async_trait;
use log::{debug, error};
use url::Url;

use crate::{
    consensus::{Participant, ValidatorStatePtr},
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};

pub struct ProtocolParticipant {
    participant_sub: MessageSubscription<Participant>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
    p2p: P2pPtr,
    channel_address: Url,
}

impl ProtocolParticipant {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        debug!("Adding ProtocolParticipant to the protocol registry");
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<Participant>().await;

        let participant_sub = channel.subscribe_msg::<Participant>().await?;
        let channel_address = channel.address();

        Ok(Arc::new(Self {
            participant_sub,
            jobsman: ProtocolJobsManager::new("ParticipantProtocol", channel),
            state,
            p2p,
            channel_address,
        }))
    }

    async fn handle_receive_participant(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolParticipant::handle_receive_participant() [START]");
        let exclude_list = vec![self.channel_address.clone()];
        loop {
            let participant = match self.participant_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!("ProtocolParticipant::handle_receive_participant(): recv error: {}", e);
                    continue
                }
            };

            debug!("ProtocolParticipant::handle_receive_participant() recv: {:?}", participant);

            let participant_copy = (*participant).clone();

            if self.state.write().await.append_participant(participant_copy.clone()) {
                if let Err(e) =
                    self.p2p.broadcast_with_exclude(participant_copy, &exclude_list).await
                {
                    error!("ProtocolParticipant::handle_receive_participant(): p2p broadcast failed: {}", e);
                };
            }
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolParticipant {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!("ProtocolParticipant::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman
            .clone()
            .spawn(self.clone().handle_receive_participant(), executor.clone())
            .await;
        debug!("ProtocolParticipant::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolParticipant"
    }
}
