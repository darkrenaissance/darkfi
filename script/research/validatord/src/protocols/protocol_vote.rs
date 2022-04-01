use async_executor::Executor;
use async_trait::async_trait;

use darkfi::{
    consensus::{state::StatePtr, vote::Vote},
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};
use log::debug;
use std::sync::Arc;

pub struct ProtocolVote {
    vote_sub: MessageSubscription<Vote>,
    jobsman: ProtocolJobsManagerPtr,
    state: StatePtr,
    p2p: P2pPtr,
}

impl ProtocolVote {
    pub async fn init(channel: ChannelPtr, state: StatePtr, p2p: P2pPtr) -> ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Vote>().await;

        let vote_sub = channel.subscribe_msg::<Vote>().await.expect("Missing Vote dispatcher!");

        Arc::new(Self {
            vote_sub,
            jobsman: ProtocolJobsManager::new("VoteProtocol", channel),
            state,
            p2p,
        })
    }

    async fn handle_receive_vote(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolVote::handle_receive_vote() [START]");
        loop {
            let vote = self.vote_sub.receive().await?;

            debug!(
                target: "ircd",
                "ProtocolVote::handle_receive_vote() received {:?}",
                vote
            );
            let vote_copy = (*vote).clone();
            if self.state.write().unwrap().receive_vote(&vote_copy) {
                self.p2p.broadcast(vote_copy).await?;
            };
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolVote {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "ircd", "ProtocolVote::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_vote(), executor.clone()).await;
        debug!(target: "ircd", "ProtocolVote::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolVote"
    }
}
