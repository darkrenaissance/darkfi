/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use async_trait::async_trait;
use darkfi::{
    event_graph::{
        self,
        proto::{EventPut, ProtocolEventGraph},
        EventGraph, EventGraphPtr,
    },
    net::{session::SESSION_DEFAULT, settings::Settings as NetSettings, P2p, P2pPtr},
    system::{sleep, Subscription},
};
use darkfi_serial::{
    deserialize_async, serialize_async, AsyncEncodable, Decodable, Encodable, SerialDecodable,
    SerialEncodable,
};
use sled_overlay::sled;
use std::{
    io::Cursor,
    sync::{Arc, Mutex as SyncMutex, OnceLock, Weak},
    time::UNIX_EPOCH,
};

use crate::{
    error::{Error, Result},
    prop::{PropertyStr, Role},
    scene::{MethodCallSub, Pimpl, SceneNodePtr, SceneNodeWeak},
    ui::{
        chatview::{MessageId, Timestamp},
        OnModify,
    },
    ExecutorPtr,
};

use super::PluginObject;

#[cfg(target_os = "android")]
mod paths {
    use crate::android::get_appdata_path;
    use std::path::PathBuf;

    pub fn get_evgrdb_path() -> PathBuf {
        get_appdata_path().join("evgr")
    }

    pub fn nick_filename() -> PathBuf {
        get_appdata_path().join("/nick.txt")
    }

    pub fn p2p_datastore_path() -> PathBuf {
        get_appdata_path().join("darkirc_p2p")
    }
    pub fn hostlist_path() -> PathBuf {
        get_appdata_path().join("hostlist.tsv")
    }
}

#[cfg(not(target_os = "android"))]
mod paths {
    use std::path::PathBuf;

    pub fn get_evgrdb_path() -> PathBuf {
        dirs::data_local_dir().unwrap().join("darkfi/evgr")
    }

    pub fn nick_filename() -> PathBuf {
        dirs::cache_dir().unwrap().join("darkfi/nick.txt")
    }

    pub fn p2p_datastore_path() -> PathBuf {
        dirs::cache_dir().unwrap().join("darkfi/wallet/darkirc_p2p")
    }
    pub fn hostlist_path() -> PathBuf {
        dirs::cache_dir().unwrap().join("darkfi/wallet/hostlist.tsv")
    }
}

use paths::*;

/// Due to drift between different machine's clocks, if the message timestamp is recent
/// then we will just correct it to the current time so messages appear sequential in the UI.
const RECENT_TIME_DIST: u64 = 25_000;

macro_rules! d { ($($arg:tt)*) => { debug!(target: "plugin::darkirc", $($arg)*); } }
macro_rules! inf { ($($arg:tt)*) => { info!(target: "plugin::darkirc", $($arg)*); } }
macro_rules! err { ($($arg:tt)*) => { error!(target: "plugin::darkirc", $($arg)*); } }

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct Privmsg {
    pub channel: String,
    pub nick: String,
    pub msg: String,
}

impl Privmsg {
    pub fn new(channel: String, nick: String, msg: String) -> Self {
        Self { channel, nick, msg }
    }

    pub fn msg_id(&self, timest: u64) -> MessageId {
        let mut hasher = blake3::Hasher::new();
        0u8.encode(&mut hasher).unwrap();
        0u8.encode(&mut hasher).unwrap();
        timest.encode(&mut hasher).unwrap();
        self.channel.encode(&mut hasher).unwrap();
        self.nick.encode(&mut hasher).unwrap();
        self.msg.encode(&mut hasher).unwrap();
        MessageId(hasher.finalize().into())
    }
}

struct SeenMsg {
    id: MessageId,
    is_self: bool,
    seen_times: usize,
}

struct SeenMessages {
    seen: Vec<SeenMsg>,
}

impl SeenMessages {
    fn new() -> Self {
        Self { seen: vec![] }
    }

    fn get_status(&self, id: &MessageId) -> Option<&SeenMsg> {
        self.seen.iter().find(|s| s.id == *id)
    }

    fn push(&mut self, id: MessageId, is_self: bool) {
        self.seen.push(SeenMsg { id, is_self, seen_times: 0 });
    }
}

pub type DarkIrcPtr = Arc<DarkIrc>;

pub struct DarkIrc {
    node: SceneNodeWeak,
    tasks: OnceLock<Vec<smol::Task<()>>>,

    p2p: P2pPtr,
    event_graph: EventGraphPtr,
    db: sled::Db,

    seen_msgs: SyncMutex<SeenMessages>,
    nick: PropertyStr,
}

impl DarkIrc {
    pub async fn new(node: SceneNodeWeak, ex: ExecutorPtr) -> Result<Pimpl> {
        let node_ref = &node.upgrade().unwrap();
        let nick = PropertyStr::wrap(node_ref, Role::Internal, "nick", 0).unwrap();

        inf!("Starting DarkIRC backend");
        let evgr_path = get_evgrdb_path();
        let db = match sled::open(&evgr_path) {
            Ok(db) => db,
            Err(err) => {
                err!("Sled database '{}' failed to open: {err}!", evgr_path.display());
                return Err(Error::SledDbErr);
            }
        };

        let mut p2p_settings: NetSettings = Default::default();
        p2p_settings.app_version = semver::Version::parse("0.5.0").unwrap();
        p2p_settings.seeds.push(url::Url::parse("tcp+tls://lilith1.dark.fi:5262").unwrap());
        p2p_settings.seeds.push(url::Url::parse("tcp+tls://agorism.dev:26661").unwrap());
        p2p_settings.seeds.push(url::Url::parse("tcp+tls://agorism.dev:26671").unwrap());
        p2p_settings.outbound_connect_timeout = 40;
        p2p_settings.channel_handshake_timeout = 30;
        p2p_settings.p2p_datastore = p2p_datastore_path().into_os_string().into_string().ok();
        p2p_settings.hostlist = hostlist_path().into_os_string().into_string().ok();

        let p2p = match P2p::new(p2p_settings, ex.clone()).await {
            Ok(p2p) => p2p,
            Err(err) => {
                err!("Create p2p network failed: {err}!");
                return Err(Error::ServiceFailed);
            }
        };

        let event_graph = match EventGraph::new(
            p2p.clone(),
            db.clone(),
            std::path::PathBuf::new(),
            false,
            "darkirc_dag",
            1,
            ex.clone(),
        )
        .await
        {
            Ok(evgr) => evgr,
            Err(err) => {
                err!("Create event graph failed: {err}!");
                return Err(Error::ServiceFailed);
            }
        };

        if let Ok(prev_nick) = std::fs::read_to_string(nick_filename()) {
            nick.set(prev_nick);
        }

        let self_ = Arc::new(Self {
            node,
            tasks: OnceLock::new(),

            p2p,
            event_graph,
            db,

            seen_msgs: SyncMutex::new(SeenMessages::new()),
            nick,
        });
        Ok(Pimpl::DarkIrc(self_))
    }

    async fn dag_sync(self: Arc<Self>) {
        inf!("Starting p2p network");
        // This usually means we cannot listen on the inbound ports
        if let Err(err) = self.p2p.clone().start().await {
            err!("Failed to start p2p network: {err}!");
            return
        }

        inf!("Waiting for some P2P connections...");
        sleep(4).await;

        loop {
            // We'll attempt to sync {sync_attempts} times
            let sync_attempts = 4;
            for i in 1..=sync_attempts {
                inf!("Syncing event DAG (attempt #{})", i);
                match self.event_graph.dag_sync().await {
                    Ok(()) => break,
                    Err(e) => {
                        if i == sync_attempts {
                            err!("Failed syncing DAG. Exiting.");
                            self.p2p.stop().await;
                            return
                        } else {
                            // TODO: Maybe at this point we should prune or something?
                            // TODO: Or maybe just tell the user to delete the DAG from FS.
                            err!("Failed syncing DAG ({}), retrying in {}s...", e, 4);
                            sleep(4).await;
                        }
                    }
                }
            }
            const sleep_time: u64 = 20;
            inf!("Wasn't able to sync yet. Sleeping for {sleep_time} and try again.");
            sleep(sleep_time).await;
        }
    }

    async fn relay_events(self: Arc<Self>, ev_sub: Subscription<event_graph::Event>) {
        loop {
            let ev = ev_sub.receive().await;

            // Try to deserialize the `Event`'s content into a `Privmsg`
            let privmsg: Privmsg = match deserialize_async(ev.content()).await {
                Ok(v) => v,
                Err(e) => {
                    err!("[IRC CLIENT] Failed deserializing incoming Privmsg event: {}", e);
                    continue
                }
            };

            let mut timest = ev.timestamp;
            let msg_id = privmsg.msg_id(timest);
            inf!(
                "Relaying ev_id={:?}, ev={ev:?}, msg_id={msg_id}, privmsg={privmsg:?}, timest={timest}",
                ev.id(),
            );

            let is_self = {
                let mut is_self = false;
                let mut seen = self.seen_msgs.lock().unwrap();
                match seen.get_status(&msg_id) {
                    Some(msg) => {
                        is_self = msg.is_self;

                        if !msg.is_self || msg.seen_times > 1 {
                            warn!(target: "plugin::darkirc", "Skipping duplicate seen message: {msg_id}");
                            continue
                        }
                    }
                    None => {
                        seen.push(msg_id.clone(), false);
                    }
                }
                is_self
            };

            // This is a hack to make messages appear sequentially in the UI
            let now_timest = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
            if !is_self && timest.abs_diff(now_timest) < RECENT_TIME_DIST {
                d!("Applied timestamp correction: <{timest}> => <{now_timest}>");
                timest = now_timest;
            }

            // Strip off starting #
            let mut channel = privmsg.channel;
            if channel.is_empty() {
                warn!(target: "plugin::darkirc", "Received privmsg with empty channel!");
                continue
            }
            if channel.chars().next().unwrap() != '#' {
                warn!(target: "plugin::darkirc", "Skipping encrypted channel: {channel}");
                continue
            }
            channel.remove(0);

            // Workaround for the chatview hack. This nick is off limits!
            let mut nick = privmsg.nick;
            if nick == "NOTICE" {
                nick = "noticer".to_string();
            }

            let mut arg_data = vec![];
            channel.encode(&mut arg_data).unwrap();
            timest.encode(&mut arg_data).unwrap();
            msg_id.encode(&mut arg_data).unwrap();
            nick.encode(&mut arg_data).unwrap();
            privmsg.msg.encode(&mut arg_data).unwrap();

            let node = self.node.upgrade().unwrap();
            node.trigger("recv", arg_data).await.unwrap();
        }
    }

    async fn process_send(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("Event relayer closed");
            return false
        };

        d!("method called: send({method_call:?})");
        assert!(method_call.send_res.is_none());

        fn decode_data(data: &[u8]) -> std::io::Result<(Timestamp, String, String)> {
            let mut cur = Cursor::new(&data);
            let timest = Timestamp::decode(&mut cur).unwrap();
            let channel = String::decode(&mut cur)?;
            let msg = String::decode(&mut cur)?;
            Ok((timest, channel, msg))
        }

        let Ok((timest, channel, msg)) = decode_data(&method_call.data) else {
            err!("send() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before send_method_task was stopped!");
        };

        self_.handle_send(timest, channel, msg).await;

        true
    }

    async fn handle_send(&self, timest: Timestamp, channel: String, msg: String) {
        let nick = self.nick.get();

        // Send text to channel
        d!("Sending privmsg: {timest} {channel}: <{nick}> {msg}");
        let msg = Privmsg::new(channel, nick, msg);
        let evgr = self.event_graph.clone();
        let mut event = event_graph::Event::new(serialize_async(&msg).await, &evgr).await;
        event.timestamp = timest;
        let msg_id = msg.msg_id(timest);

        // Keep track of our own messages so we don't apply timestamp correction to them
        // which messes up the msg id.
        {
            let mut seen = self.seen_msgs.lock().unwrap();
            seen.push(msg_id.clone(), true);
        }

        let mut arg_data = vec![];
        timest.encode_async(&mut arg_data).await.unwrap();
        msg_id.encode_async(&mut arg_data).await.unwrap();
        msg.nick.encode_async(&mut arg_data).await.unwrap();
        msg.msg.encode_async(&mut arg_data).await.unwrap();

        // Broadcast the msg

        if let Err(e) = evgr.dag_insert(&[event.clone()]).await {
            error!(target: "darkirc", "Failed inserting new event to DAG: {}", e);
        }

        self.p2p.broadcast(&EventPut(event)).await;
    }
}

#[async_trait]
impl PluginObject for DarkIrc {
    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        inf!("Registering EventGraph P2P protocol");
        let event_graph_ = Arc::clone(&self.event_graph);
        let registry = self.p2p.protocol_registry();
        registry
            .register(SESSION_DEFAULT, move |channel, _| {
                let event_graph_ = event_graph_.clone();
                async move { ProtocolEventGraph::init(event_graph_, channel).await.unwrap() }
            })
            .await;

        let me = Arc::downgrade(&self);

        let node = &self.node.upgrade().unwrap();
        let node_name = node.name.clone();
        let node_id = node.id;

        let method_sub = node.subscribe_method_call("send").unwrap();
        let me2 = me.clone();
        let send_method_task =
            ex.spawn(async move { while Self::process_send(&me2, &method_sub).await {} });

        let mut on_modify = OnModify::new(ex.clone(), node_name, node_id, me.clone());
        async fn save_nick(self_: Arc<DarkIrc>) {
            let _ = std::fs::write(nick_filename(), self_.nick.get());
        }
        on_modify.when_change(self.nick.prop(), save_nick);

        let ev_sub = self.event_graph.event_pub.clone().subscribe().await;
        let ev_task = ex.spawn(self.clone().relay_events(ev_sub));

        // Sync the DAG
        let dag_task = ex.spawn(self.clone().dag_sync());

        let mut tasks = vec![send_method_task, ev_task, dag_task];
        tasks.append(&mut on_modify.tasks);
        self.tasks.set(tasks);
    }
}
