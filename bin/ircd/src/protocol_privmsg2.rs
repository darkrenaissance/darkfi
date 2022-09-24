use std::collections::VecDeque;

use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use chrono::Utc;
use fxhash::FxHashMap;
use log::debug;
use rand::{rngs::OsRng, RngCore};

use darkfi::{
    net,
    serial::{SerialDecodable, SerialEncodable},
    util::async_util::sleep,
    Result,
};

use crate::{
    chains::{Chains, Privmsg},
    settings,
};

#[derive(Clone)]
pub struct RingBuffer<T> {
    pub items: VecDeque<T>,
}

impl<T: Eq + PartialEq + Clone> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        let items = VecDeque::with_capacity(capacity);
        Self { items }
    }

    pub fn push(&mut self, val: T) {
        if self.items.len() == self.items.capacity() {
            self.items.pop_front();
        }
        self.items.push_back(val);
    }

    pub fn contains(&self, val: &T) -> bool {
        self.items.contains(val)
    }
}

pub struct SeenIds {
    ids: Mutex<RingBuffer<String>>,
}

impl SeenIds {
    pub fn new() -> Self {
        Self { ids: Mutex::new(RingBuffer::new(settings::SIZE_OF_IDSS_BUFFER)) }
    }

    pub async fn push(&self, id: &String) -> bool {
        let ids = &mut self.ids.lock().await;
        if !ids.contains(id) {
            ids.push(id.clone());
            return true
        }
        false
    }
}

pub struct UnreadMsgs {
    msgs: Mutex<FxHashMap<String, Privmsg>>,
}

impl UnreadMsgs {
    pub fn new() -> Self {
        Self { msgs: Mutex::new(FxHashMap::default()) }
    }

    pub async fn contains(&self, key: &str) -> bool {
        self.msgs.lock().await.contains_key(key)
    }

    // Increase the read_confirms for a message, if it has exceeded the MAX_CONFIRM
    // then remove it from the hash_map and return Some(msg), otherwise return None
    pub async fn inc_read_confirms(&self, key: &str) -> Option<Privmsg> {
        let msgs = &mut self.msgs.lock().await;
        let mut result = None;

        if let Some(msg) = msgs.get_mut(key) {
            msg.read_confirms += 1;
            if msg.read_confirms >= settings::MAX_CONFIRM {
                result = Some(msg.clone())
            }
        }

        if result.is_some() {
            msgs.remove(key);
        }

        result
    }

    pub async fn insert(&self, msg: &Privmsg) {
        let msgs = &mut self.msgs.lock().await;

        // prune expired msgs
        let mut prune_ids = vec![];
        for (id, m) in msgs.iter() {
            if m.timestamp + settings::UNREAD_MSG_EXPIRE_TIME < Utc::now().timestamp() {
                prune_ids.push(id.clone());
            }
        }
        for id in prune_ids {
            msgs.remove(&id);
        }

        msgs.insert(msg.id.clone(), msg.clone());
    }
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
struct Inv {
    id: String,
    hash: String,
    target: String,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
struct GetMsgs {
    invs: Vec<String>,
    target: String,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
struct Hashes {
    hashes: Vec<String>,
    height: usize,
    target: String,
}

#[derive(SerialDecodable, SerialEncodable, Clone, Debug)]
struct SyncHash {
    height: usize,
    target: String,
}

pub struct ProtocolPrivmsg {
    jobsman: net::ProtocolJobsManagerPtr,
    notify: async_channel::Sender<Privmsg>,
    msg_sub: net::MessageSubscription<Privmsg>,
    inv_sub: net::MessageSubscription<Inv>,
    getmsgs_sub: net::MessageSubscription<GetMsgs>,
    hashes_sub: net::MessageSubscription<Hashes>,
    synchash_sub: net::MessageSubscription<SyncHash>,
    p2p: net::P2pPtr,
    channel: net::ChannelPtr,
    chains: Chains,
    seen_ids: SeenIds,
    unread_msgs: UnreadMsgs,
}

impl ProtocolPrivmsg {
    pub async fn init(
        channel: net::ChannelPtr,
        notify: async_channel::Sender<Privmsg>,
        p2p: net::P2pPtr,
        chains: Chains,
        seen_ids: SeenIds,
        unread_msgs: UnreadMsgs,
    ) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Privmsg>().await;
        message_subsytem.add_dispatch::<Inv>().await;
        message_subsytem.add_dispatch::<GetMsgs>().await;
        message_subsytem.add_dispatch::<Hashes>().await;
        message_subsytem.add_dispatch::<SyncHash>().await;

        let msg_sub =
            channel.clone().subscribe_msg::<Privmsg>().await.expect("Missing Privmsg dispatcher!");

        let inv_sub = channel.subscribe_msg::<Inv>().await.expect("Missing Inv dispatcher!");

        let getmsgs_sub =
            channel.clone().subscribe_msg::<GetMsgs>().await.expect("Missing GetMsgs dispatcher!");

        let hashes_sub =
            channel.clone().subscribe_msg::<Hashes>().await.expect("Missing Hashes dispatcher!");

        let synchash_sub = channel
            .clone()
            .subscribe_msg::<SyncHash>()
            .await
            .expect("Missing HashSync dispatcher!");

        Arc::new(Self {
            notify,
            msg_sub,
            inv_sub,
            getmsgs_sub,
            hashes_sub,
            synchash_sub,
            jobsman: net::ProtocolJobsManager::new("ProtocolPrivmsg", channel.clone()),
            p2p,
            channel,
            chains,
            seen_ids,
            unread_msgs,
        })
    }

    async fn handle_receive_inv(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_inv() [START]");
        let exclude_list = vec![self.channel.address()];
        loop {
            let inv = self.inv_sub.receive().await?;
            let inv = (*inv).to_owned();

            if !self.seen_ids.push(&inv.id).await {
                continue
            }

            // On receive inv message, if the unread_msgs buffer has the msg's hash then increase
            // the read_confirms, if not then send GetMsgs contain the msg's hash
            if !self.unread_msgs.contains(&inv.hash).await {
                self.send_getmsgs(&inv.target, vec![inv.hash.clone()]).await?;
            } else if let Some(msg) = self.unread_msgs.inc_read_confirms(&inv.hash).await {
                self.new_msg(&msg).await?;
            }

            // Either way, broadcast the inv msg
            self.p2p.broadcast_with_exclude(inv, &exclude_list).await?;
        }
    }

    async fn handle_receive_msg(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_msg() [START]");
        let exclude_list = vec![self.channel.address()];
        loop {
            let msg = self.msg_sub.receive().await?;
            let mut msg = (*msg).to_owned();

            if !self.seen_ids.push(&msg.id).await {
                continue
            }

            // If the msg has read_confirms greater or equal to MAX_CONFIRM, it will be added to
            // the chains, otherwise increase the msg's read_confirms, add it to unread_msgs, and
            // broadcast an Inv msg contain the hash of the message
            if msg.read_confirms >= settings::MAX_CONFIRM {
                self.new_msg(&msg).await?;
            } else {
                msg.read_confirms += 1;
                self.unread_msgs.insert(&msg).await;
                self.send_inv_msg(&msg).await?;
            }

            // Broadcast the msg
            self.p2p.broadcast_with_exclude(msg, &exclude_list).await?;
        }
    }

    async fn handle_receive_getmsgs(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_getmsgs() [START]");
        loop {
            let getmsgs = self.getmsgs_sub.receive().await?;

            // Load the msgs from the chains, and send them back to the sender
            let msgs = self.chains.get_msgs(&getmsgs.target, &getmsgs.invs).await;
            for msg in msgs {
                self.channel.send(msg.clone()).await?;
            }
        }
    }

    async fn handle_receive_hashes(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_hashes() [START]");
        loop {
            let hashmsg = self.hashes_sub.receive().await?;

            self.chains
                .push_hashes(hashmsg.target.clone(), hashmsg.height, hashmsg.hashes.clone())
                .await;
        }
    }

    async fn handle_receive_synchash(self: Arc<Self>) -> Result<()> {
        debug!(target: "ircd", "ProtocolPrivmsg::handle_receive_synchash() [START]");
        loop {
            let synchash = self.synchash_sub.receive().await?;

            if synchash.height < self.chains.get_height(&synchash.target).await {
                let hashes = self.chains.get_hashes(&synchash.target, synchash.height + 1).await;
                // send the hashes from the chain
                self.channel
                    .send(Hashes {
                        target: synchash.target.clone(),
                        height: synchash.height + 1,
                        hashes: hashes.clone(),
                    })
                    .await?;

                // send the msgs from the chain's buffer
                let msgs = self.chains.get_msgs(&synchash.target, &hashes).await;
                for msg in msgs {
                    self.channel.send(msg).await?;
                }
            }
        }
    }

    // every 2 seconds send a SyncHash msg, contain the last_height for each chain
    async fn send_sync_hash_loop(self: Arc<Self>) -> Result<()> {
        loop {
            // TODO loop through preconfigured channels
            let height = self.chains.get_height("").await;
            self.channel.send(SyncHash { target: "".to_string(), height }).await?;
            sleep(2).await;
        }
    }

    async fn new_msg(&self, msg: &Privmsg) -> Result<()> {
        if self.chains.push_msg(msg).await {
            self.notify.send(msg.clone()).await?;
        }
        Ok(())
    }

    async fn send_inv_msg(&self, msg: &Privmsg) -> Result<()> {
        let inv_id = OsRng.next_u64().to_string();
        self.p2p
            .broadcast(Inv { id: inv_id, hash: msg.id.clone(), target: msg.target.clone() })
            .await?;
        Ok(())
    }

    async fn send_getmsgs(&self, target: &str, hashes: Vec<String>) -> Result<()> {
        self.channel.send(GetMsgs { target: target.to_string(), invs: hashes }).await?;
        Ok(())
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
        self.jobsman.clone().spawn(self.clone().handle_receive_inv(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_getmsgs(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_hashes(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_receive_synchash(), executor.clone()).await;
        self.jobsman.clone().spawn(self.clone().send_sync_hash_loop(), executor.clone()).await;
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

impl net::Message for GetMsgs {
    fn name() -> &'static str {
        "getmsgs"
    }
}

impl net::Message for Hashes {
    fn name() -> &'static str {
        "hashes"
    }
}

impl net::Message for SyncHash {
    fn name() -> &'static str {
        "synchash"
    }
}
