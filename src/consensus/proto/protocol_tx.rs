use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use log::{debug, error, warn};

use crate::{
    consensus::{Tx, ValidatorState, ValidatorStatePtr},
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    node::MemoryState,
    util::serial::serialize,
    Result,
};

pub struct ProtocolTx {
    tx_sub: MessageSubscription<Tx>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
    p2p: P2pPtr,
}

impl ProtocolTx {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        debug!("Adding ProtocolTx to the protocol registry");
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<Tx>().await;

        let tx_sub = channel.subscribe_msg::<Tx>().await?;

        Ok(Arc::new(Self {
            tx_sub,
            jobsman: ProtocolJobsManager::new("TxProtocol", channel),
            state,
            p2p,
        }))
    }

    async fn handle_receive_tx(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolTx::handle_receive_tx() [START]");
        loop {
            let tx = match self.tx_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!("ProtocolTx::handle_receive_tx(): recv fail: {}", e);
                    continue
                }
            };

            debug!("ProtocolTx::handle_receive_tx() recv: {:?}", tx);

            let tx_copy = (*tx).clone();
            let tx_hash = blake3::hash(&serialize(&tx_copy));

            let tx_in_txstore =
                match self.state.read().await.blockchain.transactions.contains(tx_hash) {
                    Ok(v) => v,
                    Err(e) => {
                        error!("handle_receive_tx(): Failed querying txstore: {}", e);
                        continue
                    }
                };

            if self.state.read().await.unconfirmed_txs.contains(&tx_copy) || tx_in_txstore {
                debug!("ProtocolTx::handle_receive_tx(): We have already seen this tx.");
                continue
            }

            debug!("ProtocolTx::handle_receive_tx(): Starting state transition validation");
            let canon_state_clone = self.state.read().await.state_machine.lock().await.clone();
            let mem_state = MemoryState::new(canon_state_clone);
            match ValidatorState::validate_state_transitions(mem_state, &[tx_copy.clone()]) {
                Ok(_) => debug!("ProtocolTx::handle_receive_tx(): State transition valid"),
                Err(e) => {
                    warn!("ProtocolTx::handle_receive_tx(): State transition fail: {}", e);
                    continue
                }
            }

            // Nodes use unconfirmed_txs vector as seen_txs pool.
            if self.state.write().await.append_tx(tx_copy.clone()) {
                match self.p2p.broadcast(tx_copy).await {
                    Ok(()) => {}
                    Err(e) => {
                        error!("handle_receive_tx(): p2p broadcast fail: {}", e);
                        continue
                    }
                };
            }
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolTx {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!("ProtocolTx::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_tx(), executor.clone()).await;
        debug!("ProtocolTx::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolTx"
    }
}
