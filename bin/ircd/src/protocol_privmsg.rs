use async_std::sync::Mutex;
use std::{
    sync::Arc,
    collections::HashSet,
};
use log::debug;
use async_executor::Executor;
use drk::{
    net, Result,
};

use crate::privmsg::{PrivMsgId, PrivMsg};

pub struct ProtocolPrivMsg {
    notify_queue_sender: async_channel::Sender<Arc<PrivMsg>>,
    privmsg_sub: net::MessageSubscription<PrivMsg>,
    jobsman: net::ProtocolJobsManagerPtr,
    privmsg_ids: Mutex<HashSet<PrivMsgId>>,
    p2p: net::P2pPtr,
}

impl ProtocolPrivMsg {
    pub async fn new(
        channel: net::ChannelPtr,
        notify_queue_sender: async_channel::Sender<Arc<PrivMsg>>,
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
            privmsg_ids: Mutex::new(HashSet::new()),
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
            if self.privmsg_ids.lock().await.contains(&privmsg.id) {
                continue
            }

            // If not then broadcast to everybody else

            // First update list of privmsg ids
            self.privmsg_ids.lock().await.insert(privmsg.id);

            let privmsg_copy = (*privmsg).clone();
            self.p2p.broadcast(privmsg_copy).await?;

            self.notify_queue_sender.send(privmsg).await.expect("notify_queue_sender send failed!");
        }
    }
}

