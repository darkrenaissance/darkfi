use async_std::sync::Arc;

use async_executor::Executor;
use async_trait::async_trait;
use log::{debug, error, info};
use url::Url;

use crate::{
    consensus::{BlockProposal, ValidatorStatePtr},
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

            info!("ProtocolProposal::handle_receive_proposal(): recv: {}", proposal);
            debug!("ProtocolProposal::handle_receive_proposal(): Full proposal: {:?}", proposal);

            let proposal_copy = (*proposal).clone();

            if let Err(e) = self.state.write().await.receive_proposal(&proposal_copy).await {
                error!(
                    "ProtocolProposal::handle_receive_proposal(): receive_proposal error: {}",
                    e
                );
                continue
            }

            // Broadcast block to rest of nodes
            if let Err(e) = self.p2p.broadcast_with_exclude(proposal_copy, &exclude_list).await {
                error!(
                    "ProtocolProposal::handle_receive_proposal(): proposal broadcast fail: {}",
                    e
                );
            };
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
