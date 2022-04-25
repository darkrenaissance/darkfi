use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use log::debug;

use crate::{
    consensus::{ValidatorStatePtr, Vote},
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
    sync_p2p: P2pPtr,
    consensus_p2p: P2pPtr,
}

impl ProtocolVote {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        sync_p2p: P2pPtr,
        consensus_p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        debug!("Adding ProtocolVote to the protocol registry");
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<Vote>().await;

        let vote_sub = channel.subscribe_msg::<Vote>().await?;

        Ok(Arc::new(Self {
            vote_sub,
            jobsman: ProtocolJobsManager::new("VoteProtocol", channel),
            state,
            sync_p2p,
            consensus_p2p,
        }))
    }

    async fn handle_receive_vote(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolVote::handle_receive_vote() [START]");
        loop {
            let vote = self.vote_sub.receive().await?;

            debug!("ProtocolVote::handle_receive_vote() recv: {:?}", vote);

            let vote_copy = (*vote).clone();

            let (voted, to_broadcast) = self.state.write().await.receive_vote(&vote_copy)?;
            if voted {
                self.consensus_p2p.broadcast(vote_copy).await?;
                // Broadcast finalized blocks info, if any
                match to_broadcast {
                    Some(blocks) => {
                        debug!("handle_receive_vote(): Broadcasting finalized blocks");
                        for info in blocks {
                            self.sync_p2p.broadcast(info).await?;
                        }
                    }
                    None => {
                        debug!("handle_receive_vote(): No finalized blocks to broadcast");
                        continue
                    }
                }
            }
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
