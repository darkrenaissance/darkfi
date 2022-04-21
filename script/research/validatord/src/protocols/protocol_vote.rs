use async_executor::Executor;
use async_trait::async_trait;

use darkfi::{
    consensus::{state::ValidatorStatePtr, vote::Vote},
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
    state: ValidatorStatePtr,
    main_p2p: P2pPtr,
    consensus_p2p: P2pPtr,
}

impl ProtocolVote {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        main_p2p: P2pPtr,
        consensus_p2p: P2pPtr,
    ) -> ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Vote>().await;

        let vote_sub = channel.subscribe_msg::<Vote>().await.expect("Missing Vote dispatcher!");

        Arc::new(Self {
            vote_sub,
            jobsman: ProtocolJobsManager::new("VoteProtocol", channel),
            state,
            main_p2p,
            consensus_p2p,
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
            let (voted, to_broadcast) = self.state.write().unwrap().receive_vote(&vote_copy)?;
            if voted {
                self.consensus_p2p.broadcast(vote_copy).await?;
                // Broadcasting finalized blocks info, if any
                match to_broadcast {
                    Some(blocks) => {
                        for info in blocks {
                            self.main_p2p.broadcast(info).await?;
                        }
                    }
                    None => continue,
                }
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
