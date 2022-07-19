use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use darkfi::{net, Result};

use crate::dchatmsg::{Dchatmsg, DchatmsgsBuffer};
use ringbuffer::{RingBufferExt, RingBufferWrite};

pub struct ProtocolDchat {
    jobsman: net::ProtocolJobsManagerPtr,
    notify_queue_sender: async_channel::Sender<Dchatmsg>,
    msg_sub: net::MessageSubscription<Dchatmsg>,
    p2p: net::P2pPtr,
    msgs: DchatmsgsBuffer,
    channel: net::ChannelPtr,
}

impl ProtocolDchat {
    pub async fn init(
        channel: net::ChannelPtr,
        notify_queue_sender: async_channel::Sender<Dchatmsg>,
        p2p: net::P2pPtr,
        msgs: DchatmsgsBuffer,
    ) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Dchatmsg>().await;

        let msg_sub =
            channel.subscribe_msg::<Dchatmsg>().await.expect("Missing DchatMsg dispatcher!");

        Arc::new(Self {
            notify_queue_sender,
            msg_sub,
            jobsman: net::ProtocolJobsManager::new("ProtocolDchat", channel.clone()),
            p2p,
            msgs,
            channel,
        })
    }

    async fn handle_receive_msg(self: Arc<Self>) -> Result<()> {
        let exclude_list = vec![self.channel.address()];

        let msgs_buffer = self.msgs.lock().await;
        let msgs = msgs_buffer.to_vec();
        drop(msgs_buffer);
        for m in msgs {
            self.channel.send(m.clone()).await?;
        }

        loop {
            let msg = self.msg_sub.receive().await?;
            let mut msg = (*msg).to_owned();

            // add the msg to the buffer
            self.msgs.lock().await.push(msg.clone());

            self.notify_queue_sender.send(msg.clone()).await?;

            self.p2p.broadcast_with_exclude(msg, &exclude_list).await?;
        }
    }
}

#[async_trait]
impl net::ProtocolBase for ProtocolDchat {
    /// Starts ping-pong keep-alive messages exchange. Runs ping-pong in the
    /// protocol task manager, then queues the reply. Sends out a ping and
    /// waits for pong reply. Waits for ping and replies with a pong.
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_msg(), executor.clone()).await;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolDchat"
    }
}
