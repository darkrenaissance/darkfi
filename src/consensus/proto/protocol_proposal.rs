use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use log::{debug, error, warn};

use crate::{
    consensus::{BlockProposal, ValidatorState, ValidatorStatePtr},
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    node::MemoryState,
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
            let proposal = match self.proposal_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!("ProtocolProposal::handle_receive_proposal(): recv fail: {}", e);
                    continue
                }
            };

            debug!("ProtocolProposal::handle_receive_proposal() recv: {:?}", proposal);

            let proposal_copy = (*proposal).clone();

            debug!("handle_receive_proposal(): Starting state transition validation");
            let canon_state_clone = self.state.read().await.state_machine.lock().await.clone();
            let mem_state = MemoryState::new(canon_state_clone);
            match ValidatorState::validate_state_transitions(mem_state, &proposal_copy.block.txs) {
                Ok(_) => {
                    debug!("handle_receive_proposal(): State transition valid")
                }
                Err(e) => {
                    warn!("handle_receive_proposal(): State transition fail: {}", e);
                    continue
                }
            }

            let vote = self.state.write().await.receive_proposal(&proposal_copy);
            match vote {
                Ok(v) => {
                    if v.is_none() {
                        debug!("Node did not vote for the proposed block.");
                    } else {
                        let vote = v.unwrap();
                        match self.state.write().await.receive_vote(&vote) {
                            Ok(_) => {}
                            Err(e) => {
                                error!("receive_vote() error: {}", e);
                                continue
                            }
                        };
                        // Broadcast block to rest of nodes
                        match self.p2p.broadcast(proposal_copy).await {
                            Ok(()) => {}
                            Err(e) => {
                                error!("handle_receive_proposal(): proposal broadcast fail: {}", e);
                            }
                        };
                        // Broadcast vote
                        match self.p2p.broadcast(vote).await {
                            Ok(()) => {}
                            Err(e) => {
                                error!("handle_receive_proposal(): vote broadcast fail: {}", e);
                            }
                        };
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
