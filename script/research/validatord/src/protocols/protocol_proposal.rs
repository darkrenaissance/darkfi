use async_executor::Executor;
use async_trait::async_trait;

use darkfi::{
    consensus::{block::BlockProposal, state::StatePtr},
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};
use log::debug;
use std::sync::Arc;

pub struct ProtocolProposal {
    proposal_sub: MessageSubscription<BlockProposal>,
    jobsman: ProtocolJobsManagerPtr,
    state: StatePtr,
    p2p: P2pPtr,
}

impl ProtocolProposal {
    pub async fn init(channel: ChannelPtr, state: StatePtr, p2p: P2pPtr) -> ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<BlockProposal>().await;

        let proposal_sub =
            channel.subscribe_msg::<BlockProposal>().await.expect("Missing Proposal dispatcher!");

        Arc::new(Self {
            proposal_sub,
            jobsman: ProtocolJobsManager::new("ProposalProtocol", channel),
            state,
            p2p,
        })
    }

    // TODO:
    //      1. Nodes count not hardcoded.
    //      2. Remove dummy delay.
    async fn handle_receive_proposal(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolBlock::handle_receive_proposal() [START]");
        loop {
            let proposal = self.proposal_sub.receive().await?;

            debug!(
                target: "ircd",
                "ProtocolProposal::handle_receive_proposal() received {:?}",
                proposal
            );
            let proposal_copy = (*proposal).clone();
            let nodes_count = 4;
            let vote = self.state.write().unwrap().receive_proposed_block(
                &proposal_copy,
                nodes_count,
                false,
            );
            match vote {
                Ok(x) => {
                    if x.is_none() {
                        debug!("Node did not vote for the proposed block.");
                    } else {
                        let vote = x.unwrap();
                        self.state.write().unwrap().receive_vote(&vote, nodes_count as usize);
                        self.p2p.broadcast(vote).await?;
                    }
                }
                Err(e) => {
                    debug!(target: "ircd", "ProtocolBlock::handle_receive_proposal() error prosessing proposal: {:?}", e)
                }
            }
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolProposal {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "ircd", "ProtocolProposal::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_proposal(), executor.clone()).await;
        debug!(target: "ircd", "ProtocolProposal::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolProposal"
    }
}
