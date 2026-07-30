/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
    sync::{Arc, OnceLock, Weak},
    time::UNIX_EPOCH,
};

use async_lock::RwLock;
use crypto_box::{ChaChaBox, SecretKey};
use darkfi::{
    event_graph::{
        self,
        proto::{EventPut, ProtocolEventGraph},
        EventGraph, EventGraphConfig, EventGraphPtr,
    },
    net::{
        session::SESSION_DEFAULT,
        settings::{MagicBytes, NetworkProfile, Settings as NetSettings},
        ChannelPtr, P2p, P2pPtr,
    },
    system::{sleep, Subscription},
    Result as DarkFiResult,
};
use darkfi_serial::{
    deserialize_async, serialize, serialize_async, AsyncEncodable, Decodable, Encodable,
};
use irc2::{
    crypto::saltbox,
    irc::{server::MAX_NICK_LEN, IrcChannel, IrcContact},
    pad, unpad, Privmsg,
};
use parking_lot::Mutex as SyncMutex;
use sled_overlay::sled;

use crate::{
    app::schema::menu::channel::Channel,
    error::{Error, Result},
    prop::{BatchGuardPtr, PropertyAtomicGuard, PropertyStr, Role},
    scene::{MethodCallSub, Pimpl, SceneNode, SceneNodePtr, SceneNodeType, SceneNodeWeak, Slot},
    ui::{
        chatview::{MessageId, Timestamp},
        OnModify,
    },
    ExecutorPtr,
};

use super::PluginSettings;

const P2P_RETRY_TIME: u64 = 20;
const COOLOFF_SLEEP_TIME: u64 = 20;
const COOLOFF_SYNC_ATTEMPTS: usize = 6;
const SYNC_MIN_PEERS: usize = 2;

const P2P_OUTBOUND_ACTIVE: usize = 6;
const P2P_OUTBOUND_SLEEP: usize = 1;

/// Due to drift between different machine's clocks, if the message timestamp is recent
/// then we will just correct it to the current time so messages appear sequential in the UI.
const RECENT_TIME_DIST: u64 = 25_000;

// NOTE: if `paths` already lives in a shared module (e.g. `super::paths` from
// darkirc.rs's parent module), delete this block and add `use super::paths::*;`
// instead. Duplicated here so this file compiles standalone.
#[cfg(target_os = "android")]
mod paths {
    use crate::android::{get_appdata_path, get_external_storage_path};
    use std::path::PathBuf;

    pub fn get_evgrdb_path() -> PathBuf {
        get_external_storage_path().join("evgr2")
    }
    pub fn get_chatdb_path() -> PathBuf {
        get_external_storage_path().join("chatdb")
    }
    pub fn get_use_tor_filename() -> PathBuf {
        get_external_storage_path().join("use_tor.txt")
    }

    pub fn nick_filename() -> PathBuf {
        get_appdata_path().join("/nick2.txt")
    }

    pub fn p2p_datastore_path() -> PathBuf {
        get_appdata_path().join("darkirc2_p2p")
    }
    pub fn hostlist_path() -> PathBuf {
        get_appdata_path().join("hostlist2.tsv")
    }
}

#[cfg(not(target_os = "android"))]
mod paths {
    use std::path::PathBuf;

    pub fn get_evgrdb_path() -> PathBuf {
        dirs::data_local_dir().unwrap().join("darkfi/app/evgr2")
    }
    pub fn get_chatdb_path() -> PathBuf {
        dirs::data_local_dir().unwrap().join("darkfi/app/chatdb")
    }
    pub fn get_use_tor_filename() -> PathBuf {
        dirs::data_local_dir().unwrap().join("darkfi/app/use_tor.txt")
    }

    pub fn nick_filename() -> PathBuf {
        dirs::cache_dir().unwrap().join("darkfi/app/nick2.txt")
    }

    pub fn p2p_datastore_path() -> PathBuf {
        dirs::cache_dir().unwrap().join("darkfi/app/darkirc2_p2p")
    }
    pub fn hostlist_path() -> PathBuf {
        dirs::cache_dir().unwrap().join("darkfi/app/hostlist2.tsv")
    }
}

use paths::*;

macro_rules! t { ($($arg:tt)*) => { trace!(target: "plugin::darkirc2", $($arg)*); } }
macro_rules! d { ($($arg:tt)*) => { debug!(target: "plugin::darkirc2", $($arg)*); } }
macro_rules! i { ($($arg:tt)*) => { info!(target: "plugin::darkirc2", $($arg)*); } }
macro_rules! e { ($($arg:tt)*) => { error!(target: "plugin::darkirc2", $($arg)*); } }
macro_rules! w { ($($arg:tt)*) => { warn!(target: "plugin::darkirc2", $($arg)*); } }

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

pub type DarkIrc2Ptr = Arc<DarkIrc2>;

pub struct DarkIrc2 {
    node: SceneNodeWeak,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    p2p: P2pPtr,
    event_graph: EventGraphPtr,
    seen_msgs: SyncMutex<SeenMessages>,
    nick: PropertyStr,
    pub channels: RwLock<HashMap<String, IrcChannel>>,
    pub contacts: RwLock<HashMap<String, IrcContact>>,
    channels_tree: sled::Tree,
    settings: PluginSettings,
    ex: ExecutorPtr,
}

impl DarkIrc2 {
    pub async fn new(
        node: SceneNodeWeak,
        sg_root: SceneNodePtr,
        ex: ExecutorPtr,
        db: sled::Db,
    ) -> Result<Pimpl> {
        let node_ref = &node.upgrade().unwrap();
        let nick = PropertyStr::wrap(node_ref, Role::Internal, "nick", 0).unwrap();

        let setting_root = Arc::new(SceneNode::new("setting", SceneNodeType::SettingRoot));
        node_ref.link(setting_root.clone());

        i!("Starting DarkIRC backend");
        let evgr_path = get_evgrdb_path();
        let evgr_db = match sled::open(&evgr_path) {
            Ok(db) => db,
            Err(err) => {
                e!("Sled database '{}' failed to open: {err}!", evgr_path.display());
                return Err(Error::SledDbErr)
            }
        };

        let setting_tree = evgr_db.open_tree("settings")?;

        // Use the unified db for reading channels (UI stores channels there)
        let channels_tree = db.open_tree("channels")?;
        i!("Opened channels tree from unified db");

        let settings = PluginSettings { setting_root, sled_tree: setting_tree };

        let mut p2p_settings: NetSettings = Default::default();
        p2p_settings.magic_bytes = MagicBytes([251, 229, 199, 181]);
        p2p_settings.app_version = semver::Version::parse("0.5.0").unwrap();
        p2p_settings.app_name = "darkirc".to_string();
        if get_use_tor_filename().exists() {
            i!("Setup P2P network [tor]");
            let mut tor_profile = NetworkProfile::tor_default();
            tor_profile.outbound_connect_timeout = 60;
            p2p_settings.profiles.insert("tor".to_string(), tor_profile);
            p2p_settings.outbound_peer_discovery_cooloff_time = 60;

            p2p_settings.seeds.push(
                url::Url::parse(
                    "tor://g7fxelebievvpr27w7gt24lflptpw3jeeuvafovgliq5utdst6xyruyd.onion:25552",
                )
                .unwrap(),
            );
            p2p_settings.seeds.push(
                url::Url::parse(
                    "tor://yvklzjnfmwxhyodhrkpomawjcdvcaushsj6torjz2gyd7e25f3gfunyd.onion:25552",
                )
                .unwrap(),
            );
            p2p_settings.active_profiles = vec!["tor".to_string()];
        } else {
            i!("Setup P2P network [clearnet]");
            let mut profile = NetworkProfile::default();
            profile.outbound_connect_timeout = 40;
            profile.channel_handshake_timeout = 30;
            p2p_settings.profiles.insert("tcp+tls".to_string(), profile);

            p2p_settings.outbound_connections = 5;
            p2p_settings.inbound_connections = 2;

            p2p_settings.seeds.push(url::Url::parse("tcp+tls://lilith0.dark.fi:9600").unwrap());
            p2p_settings.seeds.push(url::Url::parse("tcp+tls://lilith1.dark.fi:9600").unwrap());
            p2p_settings.active_profiles = vec!["tcp+tls".to_string()];
        }
        p2p_settings.p2p_datastore = p2p_datastore_path().into_os_string().into_string().ok();
        p2p_settings.hostlist = hostlist_path().into_os_string().into_string().ok();

        settings.add_p2p_settings(&p2p_settings);

        settings.load_settings();
        settings.update_p2p_settings(&mut p2p_settings);

        let p2p = match P2p::new(p2p_settings.clone(), ex.clone()).await {
            Ok(p2p) => p2p,
            Err(err) => {
                e!("Create p2p network failed: {err}!");
                return Err(Error::ServiceFailed)
            }
        };

        let event_graph = match EventGraph::new(
            p2p.clone(),
            db.clone(),
            std::path::PathBuf::new(),
            false,
            EventGraphConfig {
                initial_genesis: 1_704_067_200_000,
                hours_rotation: 1,
                genesis_contents: b"darkirc-v1".to_vec(),
                rln_enabled: false,
                pregenerated_identity_commitments: vec![],
                max_dags: Some(24),
            },
            ex.clone(),
        )
        .await
        {
            Ok(evgr) => evgr,
            Err(err) => {
                e!("Create event graph failed: {err}!");
                return Err(Error::ServiceFailed)
            }
        };

        if let Ok(prev_nick) = std::fs::read_to_string(nick_filename()) {
            nick.set(&mut PropertyAtomicGuard::none(), prev_nick);
        }

        let self_ = Arc::new(Self {
            node: node.clone(),
            tasks: SyncMutex::new(vec![]),

            p2p,
            event_graph,

            seen_msgs: SyncMutex::new(SeenMessages::new()),
            nick,

            channels: RwLock::new(HashMap::new()),
            contacts: RwLock::new(HashMap::new()),
            channels_tree,

            settings,
            ex: ex.clone(),
        });

        self_.load_channels_from_db().await;
        self_.clone().start(sg_root, ex).await;
        Ok(Pimpl::DarkIrc2(self_))
    }

    async fn dag_sync(self: Arc<Self>, channel_sub: Subscription<DarkFiResult<ChannelPtr>>) {
        i!("Starting p2p network");
        while let Err(err) = self.p2p.clone().start().await {
            // This usually means we cannot listen on the inbound ports
            e!("Failed to start p2p network: {err}!");
            e!("Usually this means there is another process listening on the same ports.");
            e!("Trying again in {P2P_RETRY_TIME} secs");
            sleep(P2P_RETRY_TIME).await;
        }

        i!("Waiting for some P2P connections...");

        let mut sync_attempt = 0;
        // TODO: these should be configurable
        let fast_mode = false;
        let dags_count = 24;
        loop {
            if self.p2p.is_connected() {
                let peers_count = self.p2p.peers_count();
                self.notify_connect(peers_count, self.event_graph.is_synced()).await;

                // Wait until we have enough connections
                if peers_count < SYNC_MIN_PEERS {
                    i!("Connected to {peers_count} peers. Waiting for more connections.");
                    continue
                }

                i!("Got peer connection");
                sync_attempt += 1;
                // Cool off periodically
                if sync_attempt > COOLOFF_SYNC_ATTEMPTS {
                    i!("Wasn't able to sync yet. Cooling off for {COOLOFF_SLEEP_TIME} then will try again.");
                    sleep(COOLOFF_SLEEP_TIME).await;
                    sync_attempt = 0;
                }

                i!("Syncing static DAG");
                match self.event_graph.static_sync().await {
                    Ok(()) => {
                        i!("Static synced successfully");
                        // log_memory("after static sync");
                    }
                    Err(e) => {
                        e!("Failed syncing static graph: {e}");
                        self.p2p.stop().await;
                        break
                    }
                }
                i!("Syncing event DAG (attempt #{sync_attempt})");
                // Sync mode is now per-call: full sync replays
                // every event (heavy, used by archival nodes), fast
                // sync only fetches headers (light, used by clients
                // that don't need to re-verify history).
                let sync_result = if fast_mode {
                    self.event_graph.sync_selected_headers(dags_count).await
                } else {
                    self.event_graph.sync_selected(dags_count).await
                };
                match sync_result {
                    Ok(()) => {
                        i!(
                            "Event DAG synced successfully ({} mode, {} dag(s))",
                            if fast_mode { "fast" } else { "full" },
                            dags_count,
                        );
                        break
                    }
                    Err(e) => {
                        // TODO: Maybe at this point we should prune or something?
                        // TODO: Or maybe just tell the user to delete the DAG from FS.
                        e!("Failed syncing DAG ({e}), retrying...");
                    }
                }
            } else {
                i!("Waiting for some P2P connections...");
                sleep(COOLOFF_SLEEP_TIME).await;
            }
        }

        let peers_count = self.p2p.peers_count();
        self.notify_connect(peers_count, self.event_graph.is_synced()).await;

        // Initial sync finished. Now just notify of connection changes
        loop {
            // Wait for a channel
            if let Err(err) = channel_sub.receive().await {
                w!("There was an error listening for channels. The service closed unexpectedly with error: {err}");
                continue
            }

            let peers_count = self.p2p.peers_count();
            self.notify_connect(peers_count, self.event_graph.is_synced()).await;
        }
    }

    /// Send a notification when there's a change in number of peers or the DAG sync status
    pub async fn notify_connect(&self, peers_count: usize, is_dag_synced: bool) {
        let node = self.node.upgrade().unwrap();
        node.trigger("connect", serialize(&(peers_count as u32, is_dag_synced))).await.unwrap();
    }

    async fn relay_events(self: Arc<Self>, ev_sub: Subscription<event_graph::Event>) {
        loop {
            let ev = ev_sub.receive().await;

            // Try to deserialize the `Event`'s content into a `Privmsg`
            let privmsg: Privmsg = match deserialize_async(ev.content()).await {
                Ok(v) => v,
                Err(e) => {
                    e!("[IRC CLIENT] Failed deserializing incoming Privmsg event: {}", e);
                    continue
                }
            };

            // Try to decrypt messages (will decrypt encrypted channels/contacts in place)
            let mut privmsg = privmsg;
            self.try_decrypt(&mut privmsg, &self.nick.get()).await;

            let mut timest = ev.header.timestamp;
            let msg_id = msg_id(&privmsg, timest);
            t!(
                "Relaying ev_id={:?}, ev={ev:?}, msg_id={msg_id}, privmsg={privmsg:?}, timest={timest}",
                ev.id(),
            );

            let is_self = {
                let mut is_self = false;
                let mut seen = self.seen_msgs.lock();
                match seen.get_status(&msg_id) {
                    Some(msg) => {
                        is_self = msg.is_self;

                        if !msg.is_self || msg.seen_times > 1 {
                            w!("Skipping duplicate seen message: {msg_id}");
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
                w!("Received privmsg with empty channel!");
                continue
            }
            if channel.chars().next().unwrap() != '#' {
                w!("Skipping encrypted channel: {channel}");
                continue
            }

            // Workaround for the chatview hack. This nick is off limits!
            let mut nick = privmsg.nick;
            if nick == "NOTICE" {
                nick = "noticer".to_string();
            }

            self.notify_recv(channel, timest, msg_id, nick, privmsg.msg).await;
        }
    }

    /// Send a notification about a new received message
    pub async fn notify_recv(
        &self,
        channel: String,
        timestamp: Timestamp,
        id: MessageId,
        nick: String,
        msg: String,
    ) {
        let mut arg_data = vec![];
        channel.encode(&mut arg_data).unwrap();
        timestamp.encode(&mut arg_data).unwrap();
        id.encode(&mut arg_data).unwrap();
        nick.encode(&mut arg_data).unwrap();
        msg.encode(&mut arg_data).unwrap();

        let node = self.node.upgrade().unwrap();
        node.trigger("recv", arg_data).await.unwrap();
    }

    async fn process_send(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("Event relayer closed");
            return false
        };

        t!("method called: send({method_call:?})");
        assert!(method_call.send_res.is_none());

        fn decode_data(data: &[u8]) -> std::io::Result<(Timestamp, String, String)> {
            let mut cur = Cursor::new(&data);
            let timest = Timestamp::decode(&mut cur).unwrap();
            let channel = String::decode(&mut cur)?;
            let msg = String::decode(&mut cur)?;
            Ok((timest, channel, msg))
        }

        let Ok((timest, channel, msg)) = decode_data(&method_call.data) else {
            e!("send() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before send_method_task was stopped!");
        };

        self_.handle_send(timest, channel, msg).await;

        true
    }

    /// User wants to send a msg
    async fn handle_send(&self, timest: Timestamp, channel: String, msg: String) {
        let nick = self.nick.get();

        // Send text to channel
        d!("Sending privmsg: {timest} {channel}: <{nick}> {msg}");
        let mut msg = Privmsg { version: 0, msg_type: 0, channel, nick, msg };
        self.try_encrypt(&mut msg).await;
        let evgr = self.event_graph.clone();
        let event = event_graph::Event::with_timestamp(timest, serialize_async(&msg).await, &evgr)
            .await
            .unwrap();
        let msg_id = msg_id(&msg, timest);

        // Keep track of our own messages so we don't apply timestamp correction to them
        // which messes up the msg id.
        {
            let mut seen = self.seen_msgs.lock();
            seen.push(msg_id.clone(), true);
        }

        // Broadcast the msg
        let current_genesis = self.event_graph.current_genesis.read().await;
        let dag_name = current_genesis.header.timestamp.to_string();
        if let Err(e) = evgr.insert_signal_with_blob(&event, &[], &dag_name).await {
            e!("Failed inserting new event to DAG: {}", e);
        }

        if let Err(e) = self.p2p.broadcast(&EventPut(event, vec![])).await {
            e!("Event broadcast was not admitted: {e}");
        }
    }

    /// Load channels from UI database and populate encryption keys
    pub async fn load_channels_from_db(&self) {
        let mut channels = self.channels.write().await;

        for item in self.channels_tree.iter() {
            let (key, val) = item.unwrap();
            let channel_name = String::from_utf8_lossy(&key).to_string();

            // Deserialize UI channel struct
            let ui_channel = match deserialize_async::<Channel>(&val).await {
                Ok(ch) => ch,
                Err(e) => {
                    w!("Failed to deserialize channel {channel_name}: {e}");
                    continue
                }
            };

            // Convert to IrcChannel with encryption
            let full_name = format!("#{}", channel_name);
            let mut irc_channel =
                IrcChannel { topic: String::new(), nicks: HashSet::new(), saltbox: None };

            if let Some(secret) = ui_channel.secret {
                // Convert secret array to SecretKey first, then derive PublicKey
                let secret_key = SecretKey::from_bytes(secret);
                let public = secret_key.public_key();
                let saltbox = ChaChaBox::new(&public, &secret_key);

                // Log the secret in base58 for debugging
                let secret_b58 = bs58::encode(secret).into_string();
                irc_channel.saltbox = Some(Arc::new(saltbox));
            }

            let is_encrypted = irc_channel.saltbox.is_some();
            channels.insert(full_name, irc_channel);
            i!("Loaded channel: #{} (encrypted: {})", channel_name, is_encrypted);
        }
    }

    async fn rescan_channel_history(self: Arc<Self>, channel: String) {
        i!("Starting background rescan for channel: {channel}");

        // Fetch and order all events from the DAG (like darkirc does)
        let Ok(dag_events) = self.event_graph.order_events().await else {
            e!("Failed to fetch events from DAG");
            return;
        };

        let mut found_count = 0;

        for event in dag_events.iter() {
            // Deserialize Privmsg
            let mut privmsg = match deserialize_async::<Privmsg>(event.content()).await {
                Ok(pm) => pm,
                Err(e) => {
                    t!("Not a Privmsg event, skipping");
                    continue;
                }
            };

            // Try to decrypt (handles encrypted channels)
            self.try_decrypt(&mut privmsg, &self.nick.get()).await;

            // Check if message belongs to target channel
            if privmsg.channel != channel {
                continue;
            }

            found_count += 1;

            // Calculate message ID
            let timest = event.header.timestamp;
            let msg_id = msg_id(&privmsg, timest);

            // Send to ChatView via notify_recv (handles DB storage and duplicates)
            self.notify_recv(
                channel.clone(),
                timest,
                msg_id,
                privmsg.nick.clone(),
                privmsg.msg.clone(),
            )
            .await;
        }

        i!("Rescan complete for {channel}: found {found_count} messages");
    }

    async fn process_rescan(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("Rescan method closed");
            return false
        };

        t!("method called: rescan({method_call:?})");

        let Some(self_) = me.upgrade() else {
            e!("DarkIrc2 destroyed before rescan completed");
            return false
        };

        // Decode channel name from method data
        let mut cur = std::io::Cursor::new(&method_call.data);
        let Ok(channel) = String::decode(&mut cur) else {
            e!("Rescan method called with invalid channel data");
            return false
        };

        self_.load_channels_from_db().await;

        let task = self_.ex.clone().spawn(self_.clone().rescan_channel_history(channel));
        self_.tasks.lock().push(task);

        true
    }

    async fn apply_settings(self_: Arc<Self>, _: BatchGuardPtr) {
        self_.settings.save_settings();

        let p2p_settings = self_.p2p.settings();
        let mut write_guard = p2p_settings.write().await;
        self_.settings.update_p2p_settings(&mut write_guard);
    }

    async fn process_reconnect(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("Reconnect method closed");
            return false
        };

        t!("method called: reconnect({method_call:?})");

        let Some(self_) = me.upgrade() else {
            e!("DarkIrc2 destroyed before reconnect completed");
            return false
        };

        self_.handle_reconnect().await;

        true
    }

    /// User requested to reconnect
    async fn handle_reconnect(&self) {
        i!("Manual P2P reconnection triggered");
        self.p2p.clone().stop().await;

        while let Err(err) = self.p2p.clone().start().await {
            e!("Failed to start P2P network: {err}!");
            e!("Retrying in {P2P_RETRY_TIME} secs");
            sleep(P2P_RETRY_TIME).await;
        }

        i!("P2P reconnection completed");
    }

    async fn start(self: Arc<Self>, sg_root: SceneNodePtr, ex: ExecutorPtr) {
        i!("Registering EventGraph P2P protocol");
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

        let method_sub = node.subscribe_method_call("send").unwrap();
        let me2 = me.clone();
        let send_method_task =
            ex.spawn(async move { while Self::process_send(&me2, &method_sub).await {} });

        let reconnect_method_sub = node.subscribe_method_call("reconnect").unwrap();
        let me2 = me.clone();
        let reconnect_method_task =
            ex.spawn(
                async move { while Self::process_reconnect(&me2, &reconnect_method_sub).await {} },
            );

        let rescan_method_sub = node.subscribe_method_call("rescan").unwrap();
        let me2 = me.clone();
        let rescan_method_task =
            ex.spawn(async move { while Self::process_rescan(&me2, &rescan_method_sub).await {} });

        let mut on_modify = OnModify::new(ex.clone(), self.node.clone(), me.clone());
        async fn save_nick(self_: Arc<DarkIrc2>, _batch: BatchGuardPtr) {
            let _ = std::fs::write(nick_filename(), self_.nick.get());
        }
        on_modify.when_change(self.nick.prop(), save_nick);

        // `apply_settings` is triggered if any setting changes
        for setting_node in self.settings.setting_root.get_children().iter() {
            on_modify.when_change(
                setting_node.get_property("value").clone().unwrap(),
                Self::apply_settings,
            );
        }

        let ev_sub = self.event_graph.event_subscribe().await;
        let ev_task = ex.spawn(self.clone().relay_events(ev_sub));

        // Sync the DAG / check sync status
        let channel_sub = self.p2p.hosts().subscribe_channel().await;
        let dag_task = ex.spawn(self.clone().dag_sync(channel_sub));

        // Subscribe to window start/stop signals for dynamic outbound connections
        let window_node = sg_root.lookup_node("/window").unwrap();

        let (start_slot, start_recv) = Slot::new("app_start");
        window_node.register("start", start_slot).unwrap();
        let p2p = self.p2p.clone();
        let start_task = ex.spawn(async move {
            while let Ok(_) = start_recv.recv().await {
                i!("App started: set outbound connections to {P2P_OUTBOUND_ACTIVE}");
                p2p.settings().write().await.outbound_connections = P2P_OUTBOUND_ACTIVE;
                p2p.clone().reload().await;
            }
        });

        let (stop_slot, stop_recv) = Slot::new("app_stop");
        window_node.register("stop", stop_slot).unwrap();
        let p2p = self.p2p.clone();
        let stop_task = ex.spawn(async move {
            while let Ok(_) = stop_recv.recv().await {
                i!("App stopped: set outbound connections to {P2P_OUTBOUND_SLEEP}");
                p2p.settings().write().await.outbound_connections = P2P_OUTBOUND_SLEEP;
                p2p.clone().reload().await;
            }
        });

        let mut tasks = vec![
            send_method_task,
            reconnect_method_task,
            rescan_method_task,
            ev_task,
            dag_task,
            start_task,
            stop_task,
        ];
        tasks.append(&mut on_modify.tasks);
        *self.tasks.lock() = tasks;
    }

    /// Try encrypting a given `Privmsg` if there is such a channel/contact.
    pub async fn try_encrypt(&self, privmsg: &mut Privmsg) {
        if let Some((name, channel)) = self.channels.read().await.get_key_value(&privmsg.channel) {
            if let Some(saltbox) = &channel.saltbox {
                privmsg.channel = saltbox::encrypt(saltbox, &[0x00; MAX_NICK_LEN]);
                privmsg.nick = saltbox::encrypt(saltbox, &pad(&privmsg.nick));
                privmsg.msg = saltbox::encrypt(saltbox, privmsg.msg.as_bytes());
                d!("Successfully encrypted message for {name}");
                return
            }
        };

        if let Some((name, contact)) = self.contacts.read().await.get_key_value(&privmsg.channel) {
            privmsg.channel = saltbox::encrypt(&contact.saltbox, &[0x00; MAX_NICK_LEN]);
            privmsg.nick = saltbox::encrypt(&contact.self_saltbox, &[0x00; MAX_NICK_LEN]);
            privmsg.msg = saltbox::encrypt(&contact.saltbox, privmsg.msg.as_bytes());
            d!("Successfully encrypted message for {name}");
        };
    }

    /// Try decrypting a given potentially encrypted `Privmsg` object.
    pub async fn try_decrypt(&self, privmsg: &mut Privmsg, self_nickname: &str) {
        let Ok(channel_ciphertext) = bs58::decode(&privmsg.channel).into_vec() else { return };
        let Ok(nick_ciphertext) = bs58::decode(&privmsg.nick).into_vec() else { return };
        let Ok(msg_ciphertext) = bs58::decode(&privmsg.msg).into_vec() else { return };

        for (name, channel) in self.channels.read().await.iter() {
            let Some(saltbox) = &channel.saltbox else { continue };

            if saltbox::try_decrypt(saltbox, &channel_ciphertext).is_none() {
                continue
            };

            let Some(mut nick_dec) = saltbox::try_decrypt(saltbox, &nick_ciphertext) else {
                w!("Could not decrypt nick ciphertext for channel: {name}");
                continue
            };

            let Some(msg_dec) = saltbox::try_decrypt(saltbox, &msg_ciphertext) else {
                w!("Could not decrypt message ciphertext for channel: {name}");
                continue
            };

            unpad(&mut nick_dec);

            privmsg.channel = name.to_string();
            privmsg.nick = String::from_utf8_lossy(&nick_dec).into();
            privmsg.msg = String::from_utf8_lossy(&msg_dec).into();
            d!("Successfully decrypted message for {name}");
            return
        }

        for (name, contact) in self.contacts.read().await.iter() {
            if saltbox::try_decrypt(&contact.saltbox, &channel_ciphertext).is_none() {
                continue
            };

            let nick = if saltbox::try_decrypt(&contact.self_saltbox, &nick_ciphertext).is_some() {
                String::from(self_nickname)
            } else {
                name.to_string()
            };

            let Some(msg_dec) = saltbox::try_decrypt(&contact.saltbox, &msg_ciphertext) else {
                w!("Could not decrypt message ciphertext for contact: {name}");
                continue
            };

            privmsg.channel = name.to_string();
            privmsg.nick = nick;
            privmsg.msg = String::from_utf8_lossy(&msg_dec).into();
            return
        }
    }
}

pub fn msg_id(privmsg: &Privmsg, timest: u64) -> MessageId {
    let mut hasher = blake3::Hasher::new();
    0u8.encode(&mut hasher).unwrap();
    0u8.encode(&mut hasher).unwrap();
    timest.encode(&mut hasher).unwrap();
    privmsg.channel.encode(&mut hasher).unwrap();
    privmsg.nick.encode(&mut hasher).unwrap();
    privmsg.msg.encode(&mut hasher).unwrap();
    MessageId(hasher.finalize().into())
}
