use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use log::debug;

use darkfi::{
    consensus2::{block::BlockProposal, state::ValidatorStatePtr},
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};

pub struct ProtocolProposal {
    proposal_sub: MessageSubscription<BlockProposal>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
    p2p: P2pPtr,
}

impl ProtocolProposal {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        debug!("Adding ProtocolProposal to the protocol registry");
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<BlockProposal>().await;

        let proposal_sub = channel.subscribe_msg::<BlockProposal>().await?;

        Ok(Arc::new(Self {
            proposal_sub,
            jobsman: ProtocolJobsManager::new("ProposalProtocol", channel),
            state,
            p2p,
        }))
    }

    async fn handle_receive_proposal(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolProposal::handle_receive_proposal() [START]");
        loop {
            let proposal = self.proposal_sub.receive().await?;

            debug!("ProtocolProposal::handle_receive_proposal() recv: {:?}", proposal);

            let proposal_copy = (*proposal).clone();
            let vote = self.state.write().await.receive_proposal(&proposal_copy);
            match vote {
                Ok(v) => {
                    if v.is_none() {
                        debug!("Node did not vote for the proposed block.");
                    } else {
                        let vote = v.unwrap();
                        self.state.write().await.receive_vote(&vote)?;
                        // Broadcast block to rest of nodes
                        self.p2p.broadcast(proposal_copy).await?;
                        // Broadcast vote
                        self.p2p.broadcast(vote).await?;
                    }
                }
                Err(e) => {
                    debug!("ProtocolProposal::handle_receive_proposal() error processing proposal: {:?}", e);
                }
            }
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolProposal {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!("ProtocolProposal::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_proposal(), executor.clone()).await;
        debug!("ProtocolProposal::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolProposal"
    }
}
