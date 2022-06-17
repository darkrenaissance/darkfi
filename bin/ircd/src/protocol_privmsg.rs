use async_std::sync::{Arc, Mutex};

use async_executor::Executor;
use async_trait::async_trait;
use log::debug;
use url::Url;

use darkfi::{net, Result};

use crate::Privmsg;

pub struct ProtocolPrivmsg {
    jobsman: net::ProtocolJobsManagerPtr,
    notify_queue_sender: async_channel::Sender<Privmsg>,
    msg_sub: net::MessageSubscription<Privmsg>,
    p2p: net::P2pPtr,
    msg_ids: Arc<Mutex<Vec<u64>>>,
    channel_address: Url,
}

impl ProtocolPrivmsg {
    pub async fn init(
        channel: net::ChannelPtr,
        notify_queue_sender: async_channel::Sender<Privmsg>,
        p2p: net::P2pPtr,
        msg_ids: Arc<Mutex<Vec<u64>>>,
    ) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Privmsg>().await;

        let msg_sub =
            channel.subscribe_msg::<Privmsg>().await.expect("Missing Privmsg dispatcher!");
        let channel_address = channel.address();

        Arc::new(Self {
            notify_queue_sender,
            msg_sub,
            jobsman: net::ProtocolJobsManager::new("ProtocolPrivmsg", channel),
            p2p,
            msg_ids,
            channel_address,
        })
    }

    async fn handle_receive_msg(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_msg() [START]");
        let exclude_list = vec![self.channel_address.clone()];
        loop {
            let msg = self.msg_sub.receive().await?;

            if self.msg_ids.lock().await.contains(&msg.id) {
                continue
            }

            self.msg_ids.lock().await.push(msg.id);
            let msg = (*msg).clone();

            self.notify_queue_sender.send(msg.clone()).await?;

            self.p2p.broadcast_with_exclude(msg.clone(), &exclude_list).await?;
        }
    }
}

#[async_trait]
impl net::ProtocolBase for ProtocolPrivmsg {
    /// Starts ping-pong keep-alive messages exchange. Runs ping-pong in the
    /// protocol task manager, then queues the reply. Sends out a ping and
    /// waits for pong reply. Waits for ping and replies with a pong.
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_msg(), executor.clone()).await;
        debug!(target: "ircd", "ProtocolPrivmsg::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolPrivmsg"
    }
}

impl net::Message for Privmsg {
    fn name() -> &'static str {
        "privmsg"
    }
}
