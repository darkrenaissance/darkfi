use async_std::sync::Arc;
use std::cmp::Ordering;

use async_executor::Executor;
use async_trait::async_trait;
use chrono::Utc;
use log::debug;
use rand::{rngs::OsRng, RngCore};

use darkfi::{
    net,
    util::serial::{SerialDecodable, SerialEncodable},
    Result,
};

use crate::{
    buffers::{Buffers, InvSeenIds},
    Privmsg,
};

const MAX_CONFIRM: u8 = 4;
const UNREAD_MSG_EXPIRE_TIME: i64 = 18000;

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
struct Inv {
    id: u64,
    invs: Vec<InvObject>,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
pub struct LastTerm {
    pub term: u64,
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
    last_term_sub: net::MessageSubscription<LastTerm>,
    p2p: net::P2pPtr,
    channel: net::ChannelPtr,
    inv_ids: InvSeenIds,
    buffers: Buffers,
}

impl ProtocolPrivmsg {
    pub async fn init(
        channel: net::ChannelPtr,
        notify: async_channel::Sender<Privmsg>,
        p2p: net::P2pPtr,
        inv_ids: InvSeenIds,
        buffers: Buffers,
    ) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Privmsg>().await;
        message_subsytem.add_dispatch::<Inv>().await;
        message_subsytem.add_dispatch::<GetData>().await;

        let msg_sub =
            channel.clone().subscribe_msg::<Privmsg>().await.expect("Missing Privmsg dispatcher!");

        let inv_sub = channel.subscribe_msg::<Inv>().await.expect("Missing Inv dispatcher!");

        let getdata_sub =
            channel.clone().subscribe_msg::<GetData>().await.expect("Missing GetData dispatcher!");

        let last_term_sub = channel
            .clone()
            .subscribe_msg::<LastTerm>()
            .await
            .expect("Missing LastTerm dispatcher!");

        Arc::new(Self {
            notify,
            msg_sub,
            inv_sub,
            getdata_sub,
            last_term_sub,
            jobsman: net::ProtocolJobsManager::new("ProtocolPrivmsg", channel.clone()),
            p2p,
            channel,
            inv_ids,
            buffers,
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
                let msgs = &mut self.buffers.unread_msgs.lock().await.msgs;
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

            let mut msg_ids = self.buffers.seen_ids.lock().await;
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

    async fn handle_receive_last_term(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_last_term() [START]");
        loop {
            let last_term = self.last_term_sub.receive().await?;
            let last_term = last_term.term;

            self.update_unread_msgs().await?;

            let privmsgs = self.buffers.privmsgs.lock().await;
            let self_last_term = privmsgs.last_term();

            match self_last_term.cmp(&last_term) {
                Ordering::Less => {
                    for msg in privmsgs.fetch_msgs(last_term) {
                        self.channel.send(msg).await?;
                    }
                }
                Ordering::Greater | Ordering::Equal => continue,
            }
        }
    }

    async fn handle_receive_getdata(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_getdata() [START]");
        loop {
            let getdata = self.getdata_sub.receive().await?;
            let getdata = (*getdata).to_owned();

            let msgs = &self.buffers.unread_msgs.lock().await.msgs;
            for inv in getdata.invs {
                if let Some(msg) = msgs.get(&inv.0) {
                    self.channel.send(msg.clone()).await?;
                }
            }
        }
    }

    async fn add_to_unread_msgs(&self, msg: &Privmsg) -> String {
        self.buffers.unread_msgs.lock().await.insert(msg)
    }

    async fn update_unread_msgs(&self) -> Result<()> {
        let msgs = &mut self.buffers.unread_msgs.lock().await.msgs;
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
        self.buffers.privmsgs.lock().await.push(msg);
        self.notify.send(msg.clone()).await?;
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
        let msgs_buffer = self.buffers.privmsgs.lock().await;
        for m in msgs_buffer.iter() {
            self.channel.send(m.clone()).await?;
        }
        drop(msgs_buffer);

        debug!(target: "ircd", "ProtocolPrivmsg::start() [START]");
        self.jobsman.clone().start(executor.clone());
        self.jobsman.clone().spawn(self.clone().handle_receive_msg(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_inv(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_getdata(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_last_term(), executor.clone()).await;
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

impl net::Message for LastTerm {
    fn name() -> &'static str {
        "last_term"
    }
}
