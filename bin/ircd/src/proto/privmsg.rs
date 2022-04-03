use std::sync::Arc;

use async_channel::Sender;
use async_executor::Executor;
use async_std::sync::Mutex;
use async_trait::async_trait;
use fxhash::FxHashSet;
use log::debug;

use darkfi::{
    net,
    util::serial::{SerialDecodable, SerialEncodable},
    Result,
};

pub type PrivmsgId = u32;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Privmsg {
    pub id: PrivmsgId,
    pub nickname: String,
    pub channel: String,
    pub message: String,
}

impl net::Message for Privmsg {
    fn name() -> &'static str {
        "privmsg"
    }
}

pub struct SeenPrivmsgIds {
    ids: Mutex<FxHashSet<PrivmsgId>>,
}

pub type SeenPrivmsgIdsPtr = Arc<SeenPrivmsgIds>;

impl SeenPrivmsgIds {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { ids: Mutex::new(FxHashSet::default()) })
    }

    pub async fn add_seen(&self, id: u32) {
        self.ids.lock().await.insert(id);
    }

    pub async fn is_seen(&self, id: u32) -> bool {
        self.ids.lock().await.contains(&id)
    }
}

pub struct ProtocolPrivmsg {
    notify_queue_sender: Sender<Arc<Privmsg>>,
    privmsg_sub: net::MessageSubscription<Privmsg>,
    jobsman: net::ProtocolJobsManagerPtr,
    seen_ids: SeenPrivmsgIdsPtr,
    p2p: net::P2pPtr,
}

#[async_trait]
impl net::ProtocolBase for ProtocolPrivmsg {
    /// Starts ping-pong keep-alive messages exchange. Runs ping-pong in the
    /// protocol task manager, then queues the reply. Sends out a ping and
    /// waits for pong reply. Waits for ping and replies with a pong.
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivMsg::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_privmsg(), executor.clone()).await;
        debug!(target: "ircd", "ProtocolPrivmsg::start() [END]");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolPrivMsg"
    }
}

impl ProtocolPrivmsg {
    pub async fn init(
        channel: net::ChannelPtr,
        notify_queue_sender: Sender<Arc<Privmsg>>,
        seen_ids: SeenPrivmsgIdsPtr,
        p2p: net::P2pPtr,
    ) -> net::ProtocolBasePtr {
        let message_subsystem = channel.get_message_subsystem();
        message_subsystem.add_dispatch::<Privmsg>().await;

        let sub = channel.subscribe_msg::<Privmsg>().await.expect("Missing Privmsg dispatcher!");

        Arc::new(Self {
            notify_queue_sender,
            privmsg_sub: sub,
            jobsman: net::ProtocolJobsManager::new("PrivmsgProtocol", channel),
            seen_ids,
            p2p,
        })
    }

    async fn handle_receive_privmsg(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_privmsg() [START]");

        loop {
            let privmsg = self.privmsg_sub.receive().await?;

            debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_privmsg() received {:?}", privmsg);

            // Do we already have this message?
            if self.seen_ids.is_seen(privmsg.id).await {
                continue
            }

            self.seen_ids.add_seen(privmsg.id).await;

            // If not, then broadcast to network.
            let privmsg_copy = (*privmsg).clone();
            self.p2p.broadcast(privmsg_copy).await?;

            self.notify_queue_sender.send(privmsg).await.expect("notify_queue_sender send failed!");
        }
    }
}
