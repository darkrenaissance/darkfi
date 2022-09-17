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

use crate::{buffers::Buffers, settings, Privmsg};

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
    term: Option<u64>,
}

impl GetData {
    fn new(invs: Vec<InvObject>, term: Option<u64>) -> Self {
        Self { invs, term }
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
    buffers: Buffers,
}

impl ProtocolPrivmsg {
    pub async fn init(
        channel: net::ChannelPtr,
        notify: async_channel::Sender<Privmsg>,
        p2p: net::P2pPtr,
        buffers: Buffers,
    ) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Privmsg>().await;
        message_subsytem.add_dispatch::<Inv>().await;
        message_subsytem.add_dispatch::<GetData>().await;
        message_subsytem.add_dispatch::<LastTerm>().await;

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
            buffers,
        })
    }

    async fn handle_receive_inv(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_inv() [START]");
        let exclude_list = vec![self.channel.address()];
        loop {
            let inv = self.inv_sub.receive().await?;
            let inv = (*inv).to_owned();

            if !self.buffers.seen_ids.push(inv.id).await {
                continue
            }

            let mut inv_requested = vec![];
            for inv_object in inv.invs.iter() {
                if !self.buffers.unread_msgs.inc_read_confirms(&inv_object.0).await {
                    inv_requested.push(inv_object.clone());
                }
            }

            if !inv_requested.is_empty() {
                self.channel.send(GetData::new(inv_requested, None)).await?;
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

            if !self.buffers.seen_ids.push(msg.id).await {
                continue
            }

            if msg.read_confirms >= settings::MAX_CONFIRM {
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

            match self.buffers.privmsgs.last_term().await.cmp(&last_term) {
                Ordering::Greater => {
                    for msg in self.buffers.privmsgs.fetch_msgs(last_term).await {
                        self.channel.send(msg).await?;
                    }
                }
                Ordering::Less => {
                    self.channel.send(GetData::new(vec![], Some(last_term))).await?;
                }
                Ordering::Equal => continue,
            }
        }
    }

    async fn handle_receive_getdata(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_getdata() [START]");
        loop {
            let getdata = self.getdata_sub.receive().await?;
            let getdata = (*getdata).to_owned();

            for inv in getdata.invs {
                if let Some(msg) = self.buffers.unread_msgs.get(&inv.0).await {
                    self.channel.send(msg.clone()).await?;
                }
            }

            if let Some(term) = getdata.term {
                for msg in self.buffers.privmsgs.fetch_msgs(term).await {
                    self.channel.send(msg).await?;
                }
            }
        }
    }

    async fn add_to_unread_msgs(&self, msg: &Privmsg) -> String {
        self.buffers.unread_msgs.insert(msg).await
    }

    async fn update_unread_msgs(&self) -> Result<()> {
        for (hash, msg) in self.buffers.unread_msgs.load().await {
            if msg.timestamp + settings::UNREAD_MSG_EXPIRE_TIME < Utc::now().timestamp() {
                self.buffers.unread_msgs.remove(&hash).await;
                continue
            }
            if msg.read_confirms >= settings::MAX_CONFIRM {
                if let Some(msg) = self.buffers.unread_msgs.remove(&hash).await {
                    self.add_to_msgs(&msg).await?;
                }
            }
        }
        Ok(())
    }

    async fn add_to_msgs(&self, msg: &Privmsg) -> Result<()> {
        self.buffers.privmsgs.push(msg).await;
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
        for m in self.buffers.privmsgs.load().await {
            self.channel.send(m).await?;
        }

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
