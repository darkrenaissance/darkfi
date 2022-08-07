use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use log::{debug, error, info};

use crate::{
    consensus::{
        block::{BlockInfo, BlockOrder, BlockResponse},
        ValidatorStatePtr,
    },
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};

// Constant defining how many blocks we send during syncing.
const BATCH: u64 = 10;

pub struct ProtocolSync {
    channel: ChannelPtr,
    request_sub: MessageSubscription<BlockOrder>,
    block_sub: MessageSubscription<BlockInfo>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
    p2p: P2pPtr,
    consensus_mode: bool,
}

impl ProtocolSync {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        p2p: P2pPtr,
        consensus_mode: bool,
    ) -> Result<ProtocolBasePtr> {
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<BlockOrder>().await;
        msg_subsystem.add_dispatch::<BlockInfo>().await;

        let request_sub = channel.subscribe_msg::<BlockOrder>().await?;
        let block_sub = channel.subscribe_msg::<BlockInfo>().await?;

        Ok(Arc::new(Self {
            channel: channel.clone(),
            request_sub,
            block_sub,
            jobsman: ProtocolJobsManager::new("SyncProtocol", channel),
            state,
            p2p,
            consensus_mode,
        }))
    }

    async fn handle_receive_request(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolSync::handle_receive_request() [START]");
        loop {
            let order = match self.request_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!("ProtocolSync::handle_receive_request(): recv fail: {}", e);
                    continue
                }
            };

            debug!("ProtocolSync::handle_receive_request() received {:?}", order);

            // Extra validations can be added here
            let key = order.slot;
            let blocks = match self.state.read().await.blockchain.get_blocks_after(key, BATCH) {
                Ok(v) => v,
                Err(e) => {
                    error!("ProtocolSync::handle_receive_request(): get_blocks_after fail: {}", e);
                    continue
                }
            };
            debug!("ProtocolSync::handle_receive_request(): Found {} blocks", blocks.len());

            let response = BlockResponse { blocks };
            if let Err(e) = self.channel.send(response).await {
                error!("ProtocolSync::handle_receive_request(): channel send fail: {}", e)
            };
        }
    }

    async fn handle_receive_block(self: Arc<Self>) -> Result<()> {
        // Consensus-mode enabled nodes have already performed these steps,
        // during proposal finalization.
        if self.consensus_mode {
            debug!(
                "ProtocolSync::handle_receive_block(): node runs in consensus mode, skipping..."
            );
            return Ok(())
        }

        debug!("ProtocolSync::handle_receive_block() [START]");
        let exclude_list = vec![self.channel.address()];
        loop {
            let info = match self.block_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!("ProtocolSync::handle_receive_block(): recv fail: {}", e);
                    continue
                }
            };

            info!(
                "ProtocolSync::handle_receive_block(): Received block: {}",
                info.header.headerhash()
            );

            debug!("ProtocolSync::handle_receive_block(): Processing received block");
            let info_copy = (*info).clone();
            match self.state.write().await.receive_finalized_block(info_copy.clone()).await {
                Ok(v) => {
                    if v {
                        debug!("ProtocolProposal::handle_receive_block(): block processed successfully, broadcasting...");
                        if let Err(e) =
                            self.p2p.broadcast_with_exclude(info_copy, &exclude_list).await
                        {
                            error!(
                                "ProtocolSync::handle_receive_block(): p2p broadcast fail: {}",
                                e
                            );
                        };
                    }
                }
                Err(e) => {
                    debug!("ProtocolSync::handle_receive_block(): error processing finalized block: {}", e);
                }
            };
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolSync {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!("ProtocolSync::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_request(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_block(), executor.clone()).await;
        debug!("ProtocolSync::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolSync"
    }
}
