use async_executor::Executor;
use async_trait::async_trait;

use darkfi::{   
    consensus::{participant::Participant, state::ValidatorStatePtr}, 
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};
use log::debug;
use std::sync::Arc;

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
    ) -> ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Participant>().await;

        let participant_sub =
            channel.subscribe_msg::<Participant>().await.expect("Missing Participant dispatcher!");

        Arc::new(Self {
            participant_sub,
            jobsman: ProtocolJobsManager::new("ParticipantProtocol", channel),
            state,
            p2p,
        })
    }

    async fn handle_receive_participant(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolParticipant::handle_receive_participant() [START]");
        loop {
            let participant = self.participant_sub.receive().await?;

            debug!(
                target: "ircd",
                "ProtocolParticipant::handle_receive_participant() received {:?}",
                participant
            );

            let participant_copy = (*participant).clone();
            if self.state.write().unwrap().append_participant(participant_copy.clone()) {
                self.p2p.broadcast(participant_copy).await?;
            }
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolParticipant {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "ircd", "ProtocolParticipant::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman
            .clone()
            .spawn(self.clone().handle_receive_participant(), executor.clone())
            .await;
        debug!(target: "ircd", "ProtocolParticipant::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolParticipant"
    }
}
