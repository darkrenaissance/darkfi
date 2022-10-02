use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use log::debug;

use darkfi::{
    net,
    serial::{SerialDecodable, SerialEncodable},
    Result,
};

use crate::{buffers::SeenIds, Privmsg};

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
struct InvObject(String);

pub struct ProtocolPrivmsg {
    jobsman: net::ProtocolJobsManagerPtr,
    notify: async_channel::Sender<Privmsg>,
    msg_sub: net::MessageSubscription<Privmsg>,
    p2p: net::P2pPtr,
    channel: net::ChannelPtr,
    seen: Arc<Mutex<SeenIds>>,
}

impl ProtocolPrivmsg {
    pub async fn init(
        channel: net::ChannelPtr,
        notify: async_channel::Sender<Privmsg>,
        p2p: net::P2pPtr,
        seen: Arc<Mutex<SeenIds>>,
    ) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Privmsg>().await;

        let msg_sub =
            channel.clone().subscribe_msg::<Privmsg>().await.expect("Missing Privmsg dispatcher!");
        Arc::new(Self {
            notify,
            msg_sub,
            jobsman: net::ProtocolJobsManager::new("ProtocolPrivmsg", channel.clone()),
            p2p,
            channel,
            seen,
        })
    }

    async fn handle_receive_msg(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_msg() [START]");
        let exclude_list = vec![self.channel.address()];
        loop {
            let msg = self.msg_sub.receive().await?;
            let msg = (*msg).to_owned();

            {
                let ids = &mut self.seen.lock().await;
                if !ids.push(msg.id) {
                    continue
                }
            }

            self.notify.send(msg.clone()).await?;

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
