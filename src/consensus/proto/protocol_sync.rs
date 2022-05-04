use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
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
    pending: Mutex<bool>,
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
            pending: Mutex::new(false),
        }))
    }

    async fn handle_receive_request(self: Arc<Self>) -> Result<()> {
        debug!("handle_receive_request() [START]");
        loop {
            let order = match self.request_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!("ProtocolSync::handle_receive_request(): recv fail: {}", e);
                    continue
                }
            };

            debug!("handle_receive_request() received {:?}", order);

            // Extra validations can be added here
            let key = order.sl;
            let blocks = match self.state.read().await.blockchain.get_blocks_after(key, BATCH) {
                Ok(v) => v,
                Err(e) => {
                    error!("ProtocolSync::handle_receive_request(): get_blocks_after fail: {}", e);
                    continue
                }
            };
            debug!("handle_receive_request(): Found {} blocks", blocks.len());

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
            debug!("handle_receive_block(): node runs in consensus mode, skipping...");
            return Ok(())
        }
        
        debug!("handle_receive_block() [START]");
        loop {
            let info = match self.block_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!("ProtocolSync::handle_receive_block(): recv fail: {}", e);
                    continue
                }
            };

            debug!("handle_receive_block() received block");

            // We block here if there's a pending validation, otherwise we might
            // apply the same block twice.
            debug!("handle_receive_block(): Waiting for pending block to apply");
            while *self.pending.lock().await {}
            debug!("handle_receive_block(): Pending lock released");

            // Node stores finalized block, if it doesn't exist (checking by slot),
            // and removes its transactions from the unconfirmed_txs vector.            
            // Extra validations can be added here.
            *self.pending.lock().await = true;
            let info_copy = (*info).clone();

            let has_block = match self.state.read().await.blockchain.has_block(&info_copy) {
                Ok(v) => v,
                Err(e) => {
                    error!("handle_receive_block(): failed checking for has_block(): {}", e);
                    *self.pending.lock().await = false;
                    continue
                }
            };

            if !has_block {
                debug!("handle_receive_block(): Starting state transition validation");
                let canon_state_clone =
                    self.state.read().await.state_machine.lock().await.clone();
                let mem_state = MemoryState::new(canon_state_clone);
                let state_updates =
                    match ValidatorState::validate_state_transitions(mem_state, &info.txs) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("handle_receive_block(): State transition fail: {}", e);
                            *self.pending.lock().await = false;
                            continue
                        }
                    };
                debug!("handle_receive_block(): All state transitions passed");

                debug!("handle_receive_block(): Updating canon state machine");
                if let Err(e) =
                    self.state.write().await.update_canon_state(state_updates, None).await
                {
                    error!("handle_receive_block(): Canon statemachine update fail: {}", e);
                    *self.pending.lock().await = false;
                    continue
                };

                debug!("handle_receive_block(): Appending block to ledger");
                if let Err(e) = self.state.write().await.blockchain.add(&[info_copy.clone()]) {
                    error!("handle_receive_block(): blockchain.add() fail: {}", e);
                    *self.pending.lock().await = false;
                    continue
                };

                if let Err(e) = self.state.write().await.remove_txs(info_copy.txs.clone()) {
                    error!("handle_receive_block(): remove_txs() fail: {}", e);
                    *self.pending.lock().await = false;
                    continue
                };

                if let Err(e) = self.p2p.broadcast(info_copy).await {
                    error!("handle_receive_block(): p2p broadcast fail: {}", e);
                    *self.pending.lock().await = false;
                    continue
                };
            }

            *self.pending.lock().await = false;
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
