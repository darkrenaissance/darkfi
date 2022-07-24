use async_std::sync::Arc;

use async_executor::Executor;
use async_trait::async_trait;
use log::{debug, error, warn};
use url::Url;

use crate::{
    consensus::{ValidatorState, ValidatorStatePtr},
    net,
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    node::MemoryState,
    tx::Transaction,
    util::serial::serialize,
    Result,
};

pub struct ProtocolTx {
    tx_sub: MessageSubscription<Transaction>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
    p2p: P2pPtr,
    channel_address: Url,
}

impl net::Message for Transaction {
    fn name() -> &'static str {
        "tx"
    }
}

impl ProtocolTx {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        debug!("Adding ProtocolTx to the protocol registry");
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<Transaction>().await;

        let tx_sub = channel.subscribe_msg::<Transaction>().await?;
        let channel_address = channel.address();

        Ok(Arc::new(Self {
            tx_sub,
            jobsman: ProtocolJobsManager::new("TxProtocol", channel),
            state,
            p2p,
            channel_address,
        }))
    }

    async fn handle_receive_tx(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolTx::handle_receive_tx() [START]");
        let exclude_list = vec![self.channel_address.clone()];
        loop {
            let tx = match self.tx_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!("ProtocolTx::handle_receive_tx(): recv fail: {}", e);
                    continue
                }
            };

            let tx_copy = (*tx).clone();
            let tx_hash = blake3::hash(&serialize(&tx_copy));

            {
                let state = &mut self.state.write().await;
                let tx_in_txstore = match state.blockchain.transactions.contains(&tx_hash) {
                    Ok(v) => v,
                    Err(e) => {
                        error!("handle_receive_tx(): Failed querying txstore: {}", e);
                        continue
                    }
                };

                if state.unconfirmed_txs.contains(&tx_copy) || tx_in_txstore {
                    debug!("ProtocolTx::handle_receive_tx(): We have already seen this tx.");
                    continue
                }

                debug!("ProtocolTx::handle_receive_tx(): Starting state transition validation");
                let canon_state_clone = state.state_machine.lock().await.clone();
                let mem_state = MemoryState::new(canon_state_clone);
                match ValidatorState::validate_state_transitions(mem_state, &[tx_copy.clone()]) {
                    Ok(_) => debug!("ProtocolTx::handle_receive_tx(): State transition valid"),
                    Err(e) => {
                        warn!("ProtocolTx::handle_receive_tx(): State transition fail: {}", e);
                        continue
                    }
                }

                // Nodes use unconfirmed_txs vector as seen_txs pool.
                if state.append_tx(tx_copy.clone()) {
                    if let Err(e) = self.p2p.broadcast_with_exclude(tx_copy, &exclude_list).await {
                        error!("handle_receive_tx(): p2p broadcast fail: {}", e);
                    };
                }
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
