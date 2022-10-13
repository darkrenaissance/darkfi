use async_std::sync::Arc;

use async_executor::Executor;
use async_trait::async_trait;
use log::{debug, error};
use url::Url;

use crate::{
    consensus::{KeepAlive, ValidatorStatePtr},
    net::{
        ChannelPtr, MessageSubscription, P2pPtr, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    Result,
};

pub struct ProtocolKeepAlive {
    keep_alive_sub: MessageSubscription<KeepAlive>,
    jobsman: ProtocolJobsManagerPtr,
    state: ValidatorStatePtr,
    p2p: P2pPtr,
    channel_address: Url,
}

impl ProtocolKeepAlive {
    pub async fn init(
        channel: ChannelPtr,
        state: ValidatorStatePtr,
        p2p: P2pPtr,
    ) -> Result<ProtocolBasePtr> {
        debug!("Adding ProtocolKeepAlive to the protocol registry");
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<KeepAlive>().await;

        let keep_alive_sub = channel.subscribe_msg::<KeepAlive>().await?;
        let channel_address = channel.address();

        Ok(Arc::new(Self {
            keep_alive_sub,
            jobsman: ProtocolJobsManager::new("ProtocolKeepAlive", channel),
            state,
            p2p,
            channel_address,
        }))
    }

    async fn handle_receive_keep_alive(self: Arc<Self>) -> Result<()> {
        debug!("ProtocolKeepAlive::handle_receive_keep_alive() [START]");
        let exclude_list = vec![self.channel_address.clone()];
        loop {
            let keep_alive = match self.keep_alive_sub.receive().await {
                Ok(v) => v,
                Err(e) => {
                    error!("ProtocolKeepAlive::handle_receive_keep_alive(): recv error: {}", e);
                    continue
                }
            };

            debug!("ProtocolKeepAlive::handle_receive_keep_alive() recv: {:?}", keep_alive);

            let keep_alive_copy = (*keep_alive).clone();

            if self.state.write().await.participant_keep_alive(keep_alive_copy.clone()) {
                if let Err(e) =
                    self.p2p.broadcast_with_exclude(keep_alive_copy, &exclude_list).await
                {
                    error!(
                        "ProtocolKeepAlive::handle_receive_keep_alive(): p2p broadcast failed: {}",
                        e
                    );
                };
            }
        }
    }
}

#[async_trait]
impl ProtocolBase for ProtocolKeepAlive {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!("ProtocolKeepAlive::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman
            .clone()
            .spawn(self.clone().handle_receive_keep_alive(), executor.clone())
            .await;
        debug!("ProtocolKeepAlive::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolKeepAlive"
    }
}
