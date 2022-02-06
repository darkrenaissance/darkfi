use async_trait::async_trait;
use async_executor::Executor;

use darkfi::{net, Result};
use log::debug;
use std::sync::Arc;

use crate::privmsg::{PrivMsg, SeenPrivMsgIdsPtr};

pub struct ProtocolPrivMsg {
    notify_queue_sender: async_channel::Sender<Arc<PrivMsg>>,
    privmsg_sub: net::MessageSubscription<PrivMsg>,
    jobsman: net::ProtocolJobsManagerPtr,
    seen_privmsg_ids: SeenPrivMsgIdsPtr,
    p2p: net::P2pPtr,
}

impl ProtocolPrivMsg {
    pub async fn new(
        channel: net::ChannelPtr,
        notify_queue_sender: async_channel::Sender<Arc<PrivMsg>>,
        seen_privmsg_ids: SeenPrivMsgIdsPtr,
        p2p: net::P2pPtr,
    ) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<PrivMsg>().await;

        let privmsg_sub =
            channel.subscribe_msg::<PrivMsg>().await.expect("Missing PrivMsg dispatcher!");

        Arc::new(Self {
            notify_queue_sender,
            privmsg_sub,
            jobsman: net::ProtocolJobsManager::new("PrivMsgProtocol", channel),
            seen_privmsg_ids,
            p2p,
        })
    }

    async fn handle_receive_privmsg(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivMsg::handle_receive_privmsg() [START]");
        loop {
            let privmsg = self.privmsg_sub.receive().await?;

            debug!(
                target: "ircd",
                "ProtocolPrivMsg::handle_receive_privmsg() received {:?}",
                privmsg
            );

            // Do we already have this message?
            if self.seen_privmsg_ids.is_seen(privmsg.id).await {
                continue
            }

            self.seen_privmsg_ids.add_seen(privmsg.id).await;

            // If not then broadcast to everybody else
            let privmsg_copy = (*privmsg).clone();
            self.p2p.broadcast(privmsg_copy).await?;

            self.notify_queue_sender.send(privmsg).await.expect("notify_queue_sender send failed!");
        }
    }
}

#[async_trait]
impl net::ProtocolBase for ProtocolPrivMsg {
    /// Starts ping-pong keep-alive messages exchange. Runs ping-pong in the
    /// protocol task manager, then queues the reply. Sends out a ping and
    /// waits for pong reply. Waits for ping and replies with a pong.
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivMsg::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_privmsg(), executor.clone()).await;
        debug!(target: "ircd", "ProtocolPrivMsg::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolPrivMsg"
    }
}
