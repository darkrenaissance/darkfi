use async_executor::Executor;
use async_trait::async_trait;

use darkfi::{
    consensus::{
        block::{BlockInfo, BlockOrder, BlockResponse},
        state::ValidatorStatePtr,
    },
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};
use log::debug;
use std::sync::Arc;

// Constant defining how many blocks we send during syncing.
const BATCH: u64 = 10;

pub struct ProtocolSync {
    channel: ChannelPtr,
    order_sub: MessageSubscription<BlockOrder>,
    block_sub: MessageSubscription<BlockInfo>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
    _p2p: P2pPtr,
}

impl ProtocolSync {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        _p2p: P2pPtr,
    ) -> ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<BlockOrder>().await;
        message_subsytem.add_dispatch::<BlockInfo>().await;

        let order_sub =
            channel.subscribe_msg::<BlockOrder>().await.expect("Missing BlockOrder dispatcher!");
        let block_sub =
            channel.subscribe_msg::<BlockInfo>().await.expect("Missing BlockInfo dispatcher!");

        Arc::new(Self {
            channel: channel.clone(),
            order_sub,
            block_sub,
            jobsman: ProtocolJobsManager::new("SyncProtocol", channel),
            state,
            _p2p,
        })
    }

    async fn handle_receive_order(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolSync::handle_receive_tx() [START]");
        loop {
            let order = self.order_sub.receive().await?;

            debug!(
                target: "ircd",
                "ProtocolSync::handle_receive_order() received {:?}",
                order
            );

            // Extra validations can be added here.
            let key = order.sl;
            let blocks = self.state.read().unwrap().blockchain.get_with_info(key, BATCH)?;
            let response = BlockResponse { blocks };
            self.channel.send(response).await?;
        }
    }

    async fn handle_receive_block(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolSync::handle_receive_block() [START]");
        loop {
            let info = self.block_sub.receive().await?;

            debug!(
                target: "ircd",
                "ProtocolSync::handle_receive_block() received {:?}",
                info
            );

            // TODO: Following code should be executed only by replicators, not consensus nodes.
            // Commented for now, as to not mess consensus testing.
            // (Don't forget to remove _ from _p2p)
            /*
            // Node stores finalized block, if it doesn't exists (checking by slot),
            // and removes its transactions from the unconfirmed_txs vector.
            // Extra validations can be added here.
            let info_copy = (*info).clone();
            if !self.state.read().unwrap().blockchain.has_block(&info_copy)? {
                self.state.write().unwrap().blockchain.add_by_info(info_copy.clone())?;
                self.state.write().unwrap().remove_txs(info_copy.txs.clone())?;
                self.p2p.broadcast(info_copy).await?;
            }
            */
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolSync {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "ircd", "ProtocolSync::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_order(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_block(), executor.clone()).await;
        debug!(target: "ircd", "ProtocolSync::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolSync"
    }
}
