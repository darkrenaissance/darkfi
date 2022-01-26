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
    ) -> Arc<Self> {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<PrivMsg>().await;

        debug!("ADDED DISPATCH");

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

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) {
        debug!(target: "ircd", "ProtocolPrivMsg::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_privmsg(), executor.clone()).await;
        debug!(target: "ircd", "ProtocolPrivMsg::start() [END]");
    }

    async fn handle_receive_privmsg(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolAddress::handle_receive_privmsg() [START]");
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
