use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use log::debug;

use crate::{
    consensus2::{
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
    order_sub: MessageSubscription<BlockOrder>,
    block_sub: MessageSubscription<BlockInfo>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
    p2p: P2pPtr,
}

impl ProtocolSync {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<BlockOrder>().await;
        msg_subsystem.add_dispatch::<BlockInfo>().await;

        let order_sub = channel.subscribe_msg::<BlockOrder>().await?;
        let block_sub = channel.subscribe_msg::<BlockInfo>().await?;

        Ok(Arc::new(Self {
            channel: channel.clone(),
            order_sub,
            block_sub,
            jobsman: ProtocolJobsManager::new("SyncProtocol", channel),
            state,
            p2p,
        }))
    }

    async fn handle_receive_order(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolSync::handle_receive_order() [START]");
        loop {
            let order = self.order_sub.receive().await?;

            debug!("ProtocolSync::handle_receive_order() received {:?}", order);

            // Extra validations can be added here
            let key = order.sl;
            let slot_range: Vec<u64> = (key..=(key + BATCH)).collect();
            debug!("ProtocolSync::handle_receive_order(): Querying block range: {:?}", slot_range);
            let blocks = self.state.read().await.blockchain.get_blocks_by_slot(&slot_range)?;
            debug!("ProtocolSync::handle_receive_order(): Found {} blocks", blocks.len());
            let response = BlockResponse { blocks };
            self.channel.send(response).await?;
        }
    }

    async fn handle_receive_block(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolSync::handle_receive_block() [START]");
        loop {
            let info = self.block_sub.receive().await?;

            debug!("ProtocolSync::handle_receive_block() received block");

            // TODO: The following code should be executed only by replicators, not
            // consensus nodes.

            // Node stores finalized flock, if it doesn't exist (checking by slot),
            // and removes its transactions from the unconfirmed_txs vector.
            // Extra validations can be added here.
            let info_copy = (*info).clone();
            if !self.state.read().await.blockchain.has_block(&info_copy)? {
                self.state.write().await.blockchain.add(&[info_copy.clone()])?;
                self.state.write().await.remove_txs(info_copy.txs.clone())?;
                self.p2p.broadcast(info_copy).await?;
            }
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolSync {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!("ProtocolSync::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_order(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_block(), executor.clone()).await;
        debug!("ProtocolSync::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolSync"
    }
}
