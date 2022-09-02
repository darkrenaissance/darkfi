use async_std::sync::Arc;

use async_executor::Executor;
use async_trait::async_trait;
use log::debug;

use darkfi::{net, Result};

use crate::{
    buffers::{ArcPrivmsgsBuffer, SeenMsgIds},
    Privmsg,
};

pub struct ProtocolPrivmsg {
    jobsman: net::ProtocolJobsManagerPtr,
    notify_queue_sender: async_channel::Sender<Privmsg>,
    msg_sub: net::MessageSubscription<Privmsg>,
    p2p: net::P2pPtr,
    msg_ids: SeenMsgIds,
    msgs: ArcPrivmsgsBuffer,
    channel: net::ChannelPtr,
}

impl ProtocolPrivmsg {
    pub async fn init(
        channel: net::ChannelPtr,
        notify_queue_sender: async_channel::Sender<Privmsg>,
        p2p: net::P2pPtr,
        msg_ids: SeenMsgIds,
        msgs: ArcPrivmsgsBuffer,
    ) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Privmsg>().await;

        let msg_sub =
            channel.subscribe_msg::<Privmsg>().await.expect("Missing Privmsg dispatcher!");

        Arc::new(Self {
            notify_queue_sender,
            msg_sub,
            jobsman: net::ProtocolJobsManager::new("ProtocolPrivmsg", channel.clone()),
            p2p,
            msg_ids,
            msgs,
            channel,
        })
    }

    async fn handle_receive_msg(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_msg() [START]");
        let exclude_list = vec![self.channel.address()];

        // once a channel get started
        let msgs_buffer = self.msgs.lock().await;
        let msgs = msgs_buffer.to_vec();
        drop(msgs_buffer);
        for m in msgs {
            self.channel.send(m.clone()).await?;
        }

        loop {
            let msg = self.msg_sub.receive().await?;
            let msg = (*msg).to_owned();

            {
                let msg_ids = &mut self.msg_ids.lock().await;
                if msg_ids.contains(&msg.id) {
                    continue
                }

                msg_ids.push(msg.id);
            }

            // add the msg to the buffer
            self.msgs.lock().await.push(&msg);

            self.notify_queue_sender.send(msg.clone()).await?;

            self.p2p.broadcast_with_exclude(msg, &exclude_list).await?;
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
