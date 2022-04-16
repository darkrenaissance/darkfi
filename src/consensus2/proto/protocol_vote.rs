use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use log::debug;

use crate::{
    consensus2::{ValidatorStatePtr, Vote},
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};

pub struct ProtocolVote {
    vote_sub: MessageSubscription<Vote>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
    p2p: P2pPtr,
}

impl ProtocolVote {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        debug!("Adding ProtocolVote to the protocol registry");
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<Vote>().await;

        let vote_sub = channel.subscribe_msg::<Vote>().await?;

        Ok(Arc::new(Self {
            vote_sub,
            jobsman: ProtocolJobsManager::new("VoteProtocol", channel),
            state,
            p2p,
        }))
    }

    async fn handle_receive_vote(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolVote::handle_receive_vote() [START]");
        loop {
            let vote = self.vote_sub.receive().await?;

            debug!("ProtocolVote::handle_receive_vote() recv: {:?}", vote);

            let vote_copy = (*vote).clone();
            if self.state.write().await.receive_vote(&vote_copy)? {
                self.p2p.broadcast(vote_copy).await?;
            };
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolVote {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!("ProtocolVote::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_vote(), executor.clone()).await;
        debug!("ProtocolVote::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolVote"
    }
}
