use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use log::{debug, error, warn};

use crate::{
    consensus::{
        block::{BlockInfo, BlockOrder, BlockResponse},
        ValidatorState, ValidatorStatePtr,
    },
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    node::MemoryState,
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
            let order = self.request_sub.receive().await?;

            debug!("ProtocolSync::handle_receive_request() received {:?}", order);

            // Extra validations can be added here
            let key = order.sl;
            let blocks = self.state.read().await.blockchain.get_blocks_after(key, BATCH)?;
            debug!("ProtocolSync::handle_receive_request(): Found {} blocks", blocks.len());

            let response = BlockResponse { blocks };
            self.channel.send(response).await?;
        }
    }

    async fn handle_receive_block(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolSync::handle_receive_block() [START]");
        loop {
            let info = self.block_sub.receive().await?;

            debug!("ProtocolSync::handle_receive_block() received block");

            // Node stores finalized block, if it doesn't exist (checking by slot),
            // and removes its transactions from the unconfirmed_txs vector.
            // Consensus-mode enabled nodes have already performed these steps,
            // during proposal finalization.
            // Extra validations can be added here.
            if !self.consensus_mode {
                let info_copy = (*info).clone();
                if !self.state.read().await.blockchain.has_block(&info_copy)? {
                    debug!("handle_receive_block(): Starting state transition validation");
                    let canon_state_clone =
                        self.state.read().await.state_machine.lock().await.clone();
                    let mem_state = MemoryState::new(canon_state_clone);
                    let state_updates =
                        match ValidatorState::validate_state_transitions(mem_state, &info.txs) {
                            Ok(v) => v,
                            Err(e) => {
                                warn!("handle_receive_block(): State transition fail: {}", e);
                                continue
                            }
                        };
                    debug!("ProtocolSync::handle_receive_block(): All state transitions passed");

                    debug!("ProtocolSync::handle_receive_block(): Updating canon state machine");
                    match self.state.write().await.update_canon_state(state_updates, None).await {
                        Ok(()) => {}
                        Err(e) => {
                            error!("Failed updating canon state machine: {}", e);
                            continue
                        }
                    }

                    debug!("ProtocolSync::handle_receive_block(): Appending block to ledger");
                    self.state.write().await.blockchain.add(&[info_copy.clone()])?;

                    self.state.write().await.remove_txs(info_copy.txs.clone())?;
                    self.p2p.broadcast(info_copy).await?;
                }
            }
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
