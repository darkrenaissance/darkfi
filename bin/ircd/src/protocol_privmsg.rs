use std::{
    sync::Arc,
};
use log::debug;
use async_executor::Executor;
use drk::{
    net, Result,
};

use crate::privmsg::PrivMsg;

pub struct ProtocolPrivMsg {
    notify_queue_sender: async_channel::Sender<Arc<PrivMsg>>,
    privmsg_sub: net::MessageSubscription<PrivMsg>,
    jobsman: net::ProtocolJobsManagerPtr,
}

impl ProtocolPrivMsg {
    pub async fn new(
        channel: net::ChannelPtr,
        notify_queue_sender: async_channel::Sender<Arc<PrivMsg>>,
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

            self.notify_queue_sender.send(privmsg).await.expect("notify_queue_sender send failed!");
        }
    }
}

