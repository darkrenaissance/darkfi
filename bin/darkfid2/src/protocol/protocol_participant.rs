use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use log::debug;

use darkfi::{
    consensus2::{state::ValidatorStatePtr, Participant},
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
}

impl ProtocolParticipant {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<Participant>().await;

        let participant_sub = channel.subscribe_msg::<Participant>().await?;

        Ok(Arc::new(Self {
            participant_sub,
            jobsman: ProtocolJobsManager::new("ParticipantProtocol", channel),
            state,
            p2p,
        }))
    }

    async fn handle_receive_participant(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolParticipant::handle_receive_participant() [START]");
        loop {
            let participant = self.participant_sub.receive().await?;

            debug!("ProtocolParticipant::handle_receive_participant() recv: {:?}", participant);

            if self.state.write().await.append_participant((*participant).clone()) {
                let pending_participants =
                    self.state.read().await.consensus.pending_participants.clone();
                for participant in pending_participants {
                    self.p2p.broadcast(participant).await?;
                }
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
