use async_std::sync::Arc;

use async_executor::Executor;
use async_trait::async_trait;
use log::{debug, error, warn};
use url::Url;

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
    channel_address: Url,
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

        let channel_address = channel.address();

        Ok(Arc::new(Self {
            proposal_sub,
            jobsman: ProtocolJobsManager::new("ProposalProtocol", channel),
            state,
            p2p,
            channel_address,
        }))
    }

    async fn handle_receive_proposal(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolProposal::handle_receive_proposal() [START]");

        let exclude_list = vec![self.channel_address.clone()];
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
                Ok(_) => debug!("handle_receive_proposal(): State transition valid"),
                Err(e) => {
                    warn!("handle_receive_proposal(): State transition fail: {}", e);
                    continue
                }
            }

            let vote = match self.state.write().await.receive_proposal(&proposal_copy) {
                Ok(v) => {
                    if v.is_none() {
                        debug!("handle_receive_proposal(): Node didn't vote for proposed block.");
                        continue
                    }
                    v.unwrap()
                }
                Err(e) => {
                    debug!("ProtocolProposal::handle_receive_proposal(): error processing proposal: {}", e);
                    continue
                }
            };

            if let Err(e) = self.state.write().await.receive_vote(&vote).await {
                error!("handle_receive_proposal(): receive_vote error: {}", e);
                continue
            }

            // Broadcast block to rest of nodes
            if let Err(e) = self.p2p.broadcast_with_exclude(proposal_copy, &exclude_list).await {
                error!("handle_receive_proposal(): proposal broadcast fail: {}", e);
            };

            // Broadcast vote
            if let Err(e) = self.p2p.broadcast(vote).await {
                error!("handle_receive_proposal(): vote broadcast fail: {}", e);
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
