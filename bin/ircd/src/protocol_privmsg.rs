use async_std::sync::Arc;

use async_executor::Executor;
use async_trait::async_trait;
use chrono::Utc;
use log::debug;
use rand::{rngs::OsRng, RngCore};
use ripemd::{Digest, Ripemd160};

use darkfi::{
    net,
    util::{
        serial::{SerialDecodable, SerialEncodable},
        sleep,
    },
    Result,
};

use crate::{
    buffers::{ArcPrivmsgsBuffer, SeenIds},
    Privmsg, UnreadMsgs,
};

const MAX_CONFIRM: u8 = 4;
const SLEEP_TIME_FOR_RESEND: u64 = 1200;
const UNREAD_MSG_EXPIRE_TIME: i64 = 259200;

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
struct Inv {
    id: u64,
    invs: Vec<InvObject>,
}

impl Inv {
    fn new(invs: Vec<InvObject>) -> Self {
        let id = OsRng.next_u64();
        Self { id, invs }
    }
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
struct GetData {
    invs: Vec<InvObject>,
}

impl GetData {
    fn new(invs: Vec<InvObject>) -> Self {
        Self { invs }
    }
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
struct InvObject(String);

pub struct ProtocolPrivmsg {
    jobsman: net::ProtocolJobsManagerPtr,
    notify: async_channel::Sender<Privmsg>,
    msg_sub: net::MessageSubscription<Privmsg>,
    inv_sub: net::MessageSubscription<Inv>,
    getdata_sub: net::MessageSubscription<GetData>,
    p2p: net::P2pPtr,
    msg_ids: SeenIds,
    inv_ids: SeenIds,
    msgs: ArcPrivmsgsBuffer,
    unread_msgs: UnreadMsgs,
    channel: net::ChannelPtr,
}

impl ProtocolPrivmsg {
    pub async fn init(
        channel: net::ChannelPtr,
        notify: async_channel::Sender<Privmsg>,
        p2p: net::P2pPtr,
        msg_ids: SeenIds,
        inv_ids: SeenIds,
        msgs: ArcPrivmsgsBuffer,
        unread_msgs: UnreadMsgs,
    ) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Privmsg>().await;
        message_subsytem.add_dispatch::<Inv>().await;
        message_subsytem.add_dispatch::<GetData>().await;

        let msg_sub =
            channel.clone().subscribe_msg::<Privmsg>().await.expect("Missing Privmsg dispatcher!");

        let getdata_sub =
            channel.clone().subscribe_msg::<GetData>().await.expect("Missing GetData dispatcher!");

        let inv_sub = channel.subscribe_msg::<Inv>().await.expect("Missing Inv dispatcher!");

        Arc::new(Self {
            notify,
            msg_sub,
            inv_sub,
            getdata_sub,
            jobsman: net::ProtocolJobsManager::new("ProtocolPrivmsg", channel.clone()),
            p2p,
            msg_ids,
            inv_ids,
            msgs,
            unread_msgs,
            channel,
        })
    }

    async fn handle_receive_inv(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_inv() [START]");
        let exclude_list = vec![self.channel.address()];
        loop {
            let inv = self.inv_sub.receive().await?;
            let inv = (*inv).to_owned();

            let mut inv_ids = self.inv_ids.lock().await;
            if inv_ids.contains(&inv.id) {
                continue
            }
            inv_ids.push(inv.id);
            drop(inv_ids);

            let mut inv_requested = vec![];
            for inv_object in inv.invs.iter() {
                let mut msgs = self.unread_msgs.lock().await;
                if let Some(msg) = msgs.get_mut(&inv_object.0) {
                    msg.read_confirms += 1;
                } else {
                    inv_requested.push(inv_object.clone());
                }
            }

            if !inv_requested.is_empty() {
                self.channel.send(GetData::new(inv_requested)).await?;
            }

            self.update_unread_msgs().await?;

            self.p2p.broadcast_with_exclude(inv, &exclude_list).await?;
        }
    }

    async fn handle_receive_msg(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_msg() [START]");
        let exclude_list = vec![self.channel.address()];
        loop {
            let msg = self.msg_sub.receive().await?;
            let mut msg = (*msg).to_owned();

            let mut msg_ids = self.msg_ids.lock().await;
            if msg_ids.contains(&msg.id) {
                continue
            }
            msg_ids.push(msg.id);
            drop(msg_ids);

            if msg.read_confirms >= MAX_CONFIRM {
                self.add_to_msgs(&msg).await?;
            } else {
                msg.read_confirms += 1;
                let hash = self.add_to_unread_msgs(&msg).await;
                self.p2p.broadcast(Inv::new(vec![InvObject(hash)])).await?;
            }

            self.update_unread_msgs().await?;
            self.p2p.broadcast_with_exclude(msg, &exclude_list).await?;
        }
    }

    async fn handle_receive_getdata(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_getdata() [START]");
        loop {
            let getdata = self.getdata_sub.receive().await?;
            let getdata = (*getdata).to_owned();

            let msgs = self.unread_msgs.lock().await;
            for inv in getdata.invs {
                if let Some(msg) = msgs.get(&inv.0) {
                    self.channel.send(msg.clone()).await?;
                }
            }
        }
    }

    async fn add_to_unread_msgs(&self, msg: &Privmsg) -> String {
        let mut msgs = self.unread_msgs.lock().await;
        let mut hasher = Ripemd160::new();
        hasher.update(msg.to_string());
        let key = hex::encode(hasher.finalize());
        msgs.insert(key.clone(), msg.clone());
        key
    }

    async fn update_unread_msgs(&self) -> Result<()> {
        let mut msgs = self.unread_msgs.lock().await;
        for (hash, msg) in msgs.clone() {
            if msg.timestamp + UNREAD_MSG_EXPIRE_TIME < Utc::now().timestamp() {
                msgs.remove(&hash);
                continue
            }
            if msg.read_confirms >= MAX_CONFIRM {
                self.add_to_msgs(&msg).await?;
                msgs.remove(&hash);
            }
        }
        Ok(())
    }

    async fn add_to_msgs(&self, msg: &Privmsg) -> Result<()> {
        self.msgs.lock().await.push(msg);
        self.notify.send(msg.clone()).await?;
        Ok(())
    }

    async fn resend_loop(self: Arc<Self>) -> Result<()> {
        sleep(SLEEP_TIME_FOR_RESEND).await;

        self.update_unread_msgs().await?;

        for msg in self.unread_msgs.lock().await.values() {
            self.channel.send(msg.clone()).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl net::ProtocolBase for ProtocolPrivmsg {
    /// Starts ping-pong keep-alive messages exchange. Runs ping-pong in the
    /// protocol task manager, then queues the reply. Sends out a ping and
    /// waits for pong reply. Waits for ping and replies with a pong.
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        // once a channel get started
        let msgs_buffer = self.msgs.lock().await;
        for m in msgs_buffer.iter() {
            self.channel.send(m.clone()).await?;
        }
        drop(msgs_buffer);

        debug!(target: "ircd", "ProtocolPrivmsg::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_msg(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_inv(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_getdata(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().resend_loop(), executor.clone()).await;
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

impl net::Message for Inv {
    fn name() -> &'static str {
        "inv"
    }
}

impl net::Message for GetData {
    fn name() -> &'static str {
        "getdata"
    }
}
