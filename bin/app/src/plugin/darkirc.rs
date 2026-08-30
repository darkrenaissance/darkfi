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
    sync::{atomic::Ordering, Arc, OnceLock, Weak},
    time::UNIX_EPOCH,
};

use async_lock::RwLock;
use crypto_box::{ChaChaBox, PublicKey, SecretKey};
use darkfi::{
    event_graph::{
        self,
        proto::{EventPut, ProtocolEventGraph},
        EventGraph, EventGraphConfig, EventGraphPtr,
    },
    net::{
        dnet::DnetEvent,
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
use kvdb_overlay::Database as KvDb;
use parking_lot::Mutex as SyncMutex;

use crate::{
    app::schema::{
        ensure_joined_channels_seeded,
        menu::{channel::Channel, contact::Contact},
        read_joined_channels,
    },
    db::AppDbPtr,
    error::{Error, Result},
    prop::{
        BatchGuardPtr, PropertyAtomicGuard, PropertyBool, PropertyEnum, PropertyPtr, PropertyStr,
        Role,
    },
    scene::{MethodCallSub, Pimpl, SceneNodePtr, SceneNodeWeak, Slot},
    ui::{
        chatview::{MessageId, Timestamp},
        OnModify,
    },
    ExecutorPtr,
};

const P2P_RETRY_TIME: u64 = 20;
const COOLOFF_SLEEP_TIME: u64 = 20;
const COOLOFF_SYNC_ATTEMPTS: usize = 6;
const SYNC_MIN_PEERS: usize = 2;

/// Event graph rotation: `max_dags` hourly slots (`hours_rotation: 1`).
const DAGS_COUNT: u64 = 24;
/// Milliseconds in one rotation period (1 hour).
const HOUR_MS: u64 = 3_600_000;

pub(crate) const P2P_OUTBOUND_ACTIVE: usize = 3;
const P2P_OUTBOUND_SLEEP: usize = 1;

/// Update `outbound_peers` property useful for diagnostics
const DNET_ENABLED: bool = true;

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

    pub fn get_chatdb_path() -> PathBuf {
        get_external_storage_path().join("chatdb")
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

    pub fn get_chatdb_path() -> PathBuf {
        dirs::data_local_dir().unwrap().join("darkfi/app/chatdb")
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

pub type DarkIrcPtr = Arc<DarkIrc>;

pub struct DarkIrc {
    node: SceneNodeWeak,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    p2p: P2pPtr,
    event_graph: EventGraphPtr,
    seen_msgs: SyncMutex<SeenMessages>,
    nick: PropertyStr,
    chat_is_enabled: PropertyBool,
    net_transport: PropertyEnum,
    pub channels: RwLock<HashMap<String, IrcChannel>>,
    pub contacts: RwLock<HashMap<String, IrcContact>>,
    app_db: AppDbPtr,
    dm_secret: SecretKey,
    ex: ExecutorPtr,
}

impl DarkIrc {
    pub async fn new(
        node: SceneNodeWeak,
        sg_root: SceneNodePtr,
        ex: ExecutorPtr,
        kv_db: KvDb,
        app_db: AppDbPtr,
    ) -> Result<Pimpl> {
        let node_ref = &node.upgrade().unwrap();
        let nick = PropertyStr::wrap(node_ref, Role::Internal, "nick", 0).unwrap();
        let setting_node = sg_root.lookup_node("/setting").unwrap();
        let chat_is_enabled =
            PropertyBool::wrap(&setting_node, Role::User, "chat.is_enabled", 0).unwrap();
        let net_transport =
            PropertyEnum::wrap(&setting_node, Role::Internal, "net.transport", 0).unwrap();

        i!("Starting DarkIRC backend");

        let dm_secret_bytes = app_db.dm_secret().await?;
        let dm_secret = SecretKey::from_bytes(dm_secret_bytes);
        let dm_public_b58 = bs58::encode(dm_secret.public_key().to_bytes()).into_string();
        // Expose our DM public key on the plugin node so it can be displayed/shared.
        node_ref
            .set_property_str(
                &mut PropertyAtomicGuard::none(),
                Role::Internal,
                "dm_public",
                &dm_public_b58,
            )
            .unwrap();
        i!("DM identity public key (share with contacts): {dm_public_b58}");

        let mut p2p_settings: NetSettings = Default::default();
        p2p_settings.magic_bytes = MagicBytes([251, 229, 199, 181]);
        p2p_settings.app_version = semver::Version::parse("0.5.0").unwrap();
        p2p_settings.app_name = "darkirc".to_string();
        p2p_settings.inbound_connections = 2;
        let transport = net_transport.get();
        Self::apply_transport_settings(&mut p2p_settings, &transport);
        p2p_settings.p2p_datastore = p2p_datastore_path().into_os_string().into_string().ok();
        p2p_settings.hostlist = hostlist_path().into_os_string().into_string().ok();

        let p2p = match P2p::new(p2p_settings.clone(), ex.clone()).await {
            Ok(p2p) => p2p,
            Err(err) => {
                e!("Create p2p network failed: {err}!");
                return Err(Error::ServiceFailed)
            }
        };

        if DNET_ENABLED {
            i!("Enabling dnet outbound-slot event stream for outbound_peers property");
            p2p.dnet_enable();
        }

        let event_graph = match EventGraph::new(
            p2p.clone(),
            kv_db.clone(),
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

        if let Some(prev_nick) = app_db.nick_get().await? {
            nick.set(&mut PropertyAtomicGuard::none(), prev_nick);
        }

        let self_ = Arc::new(Self {
            node: node.clone(),
            tasks: SyncMutex::new(vec![]),

            p2p,
            event_graph,

            seen_msgs: SyncMutex::new(SeenMessages::new()),
            nick,
            chat_is_enabled,
            net_transport,

            channels: RwLock::new(HashMap::new()),
            contacts: RwLock::new(HashMap::new()),
            app_db,
            dm_secret,

            ex: ex.clone(),
        });

        self_.p2p.settings().write().await.outbound_connections = P2P_OUTBOUND_ACTIVE;

        ensure_joined_channels_seeded();
        self_.load_channels_from_db().await;
        self_.load_contacts_from_db().await;
        self_.clone().start(sg_root, ex).await;
        Ok(Pimpl::DarkIrc(self_))
    }

    async fn dag_sync(self: Arc<Self>, channel_sub: Subscription<DarkFiResult<ChannelPtr>>) {
        i!("Waiting for some P2P connections...");

        let mut sync_attempt = 0;
        // TODO: these should be configurable
        let fast_mode = false;
        let mut newest_synced = false;
        loop {
            let peers_count = self.p2p.peers_count();
            self.notify_connect(peers_count, self.event_graph.is_synced()).await;

            // Wait until we have enough connections
            if peers_count < SYNC_MIN_PEERS {
                i!("Connected to {peers_count} peers. Waiting for more connections.");
                let conn_sub = self.p2p.hosts().subscribe_channel().await;
                loop {
                    if let Err(err) = conn_sub.receive().await {
                        w!("Error while waiting for new connections: {err}");
                        continue
                    }

                    if self.p2p.peers_count() >= SYNC_MIN_PEERS {
                        break
                    }
                }
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
            // Sync only the newest DAG first. Older DAGs are caught up in
            // the background by `catch_up_sync`, so the `synced` flag (and
            // the `connect` notification) is reached after the first DAG
            // instead of after all `DAGS_COUNT` of them.
            let latest_ts = self.event_graph.current_genesis.read().await.header.timestamp;
            i!("Syncing newest event DAG ({latest_ts}) (attempt #{sync_attempt})");
            let sync_result = self.sync_dag_slot(latest_ts, fast_mode).await;
            match sync_result {
                Ok(()) => {
                    i!(
                        "Newest event DAG synced successfully ({} mode)",
                        if fast_mode { "fast" } else { "full" },
                    );
                    newest_synced = true;
                    break
                }
                Err(e) => {
                    // TODO: Maybe at this point we should prune or something?
                    // TODO: Or maybe just tell the user to delete the DAG from FS.
                    e!("Failed syncing newest DAG ({e}), retrying...");
                }
            }
        }

        // The newest DAG is synced: flip the `synced` flag so live `EventPut`
        // ingestion and the netstatus UI unblock, then let the remaining older
        // DAGs catch up in the background.
        if newest_synced {
            self.event_graph.synced.store(true, Ordering::Release);
            i!("Marked event graph as synced (newest DAG done)");

            let self_ = self.clone();
            let catch_up_task =
                self.ex.clone().spawn(async move { self_.catch_up_sync(fast_mode).await });
            self.tasks.lock().push(catch_up_task);
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

    /// Sync a single DAG slot, choosing `dag_sync_headers` (fast) or `dag_sync`
    /// (full) per the per-call `fast_mode` flag. Shared by the foreground
    /// newest-DAG phase and the background catch-up phase.
    async fn sync_dag_slot(&self, dag_ts: u64, fast_mode: bool) -> DarkFiResult<()> {
        if fast_mode {
            self.event_graph.dag_sync_headers(dag_ts).await
        } else {
            self.event_graph.dag_sync(dag_ts).await
        }
    }

    /// Background catch-up phase: after the newest DAG has synced (phase 1),
    /// walk the remaining DAG slots newest-to-oldest and sync each one.
    ///
    /// Per-slot failures are retried across rounds with `COOLOFF_SLEEP_TIME`
    /// pauses (no give-up), while the walk keeps making progress on the other
    /// pending slots. `current_genesis` is re-read before every retry round so
    /// rotation mid-catch-up is handled: a newer slot that rotates in is synced
    /// first, and a pending slot that rotates out of the retention window is
    /// dropped from the walk.
    async fn catch_up_sync(self: Arc<Self>, fast_mode: bool) {
        i!("Starting background catch-up of older DAGs (newest-to-oldest)");

        // The newest slot is already synced by phase 1; collect the rest of the
        // retention window below it.
        let mut last_newest = self.event_graph.current_genesis.read().await.header.timestamp;
        let mut pending: Vec<u64> =
            (1..DAGS_COUNT).map(|i| last_newest.saturating_sub(i * HOUR_MS)).collect();

        loop {
            // Re-read `current_genesis` before each retry round: rotation may
            // have shifted the newest slot or dropped a pending slot.
            let latest_ts = self.event_graph.current_genesis.read().await.header.timestamp;

            // A newer DAG rotated in: it is the new newest slot and must sync
            // before the remaining older pending DAGs.
            if latest_ts > last_newest {
                i!("Syncing newly rotated-in DAG ({latest_ts})");
                loop {
                    match self.sync_dag_slot(latest_ts, fast_mode).await {
                        Ok(()) => break,
                        Err(e) => {
                            e!("Failed syncing newly rotated-in DAG ({latest_ts}): {e}, cooling off");
                            sleep(COOLOFF_SLEEP_TIME).await;
                        }
                    }
                }
                last_newest = latest_ts;
                pending = (1..DAGS_COUNT).map(|i| latest_ts.saturating_sub(i * HOUR_MS)).collect();
            }

            // Clamp the walk to the retention window: slots rotated out fall
            // below the horizon and are skipped rather than retried forever.
            let horizon = latest_ts.saturating_sub((DAGS_COUNT - 1) * HOUR_MS);
            let mut still_pending = Vec::new();
            for dag_ts in pending.iter().copied() {
                if dag_ts < horizon {
                    i!("Skipping rotated-out DAG ({dag_ts})");
                    continue
                }

                match self.sync_dag_slot(dag_ts, fast_mode).await {
                    Ok(()) => {
                        i!("Synced older DAG ({dag_ts})");
                    }
                    Err(e) => {
                        e!("Failed syncing older DAG ({dag_ts}): {e}, will retry after cooldown");
                        still_pending.push(dag_ts);
                        sleep(COOLOFF_SLEEP_TIME).await;
                    }
                }
            }

            pending = still_pending;
            if pending.is_empty() {
                i!("Background catch-up complete; all older DAGs synced");
                break
            }
        }
    }

    /// Send a notification when there's a change in number of peers or the DAG sync status
    ///
    /// The node is only borrowed to grab the signal, so no strong node reference
    /// is held across the trigger below. Otherwise a notify racing `SceneNode::setup`'s
    /// strong_count assertion would panic the app.
    pub async fn notify_connect(&self, peers_count: usize, is_dag_synced: bool) {
        let sig = {
            let node = self.node.upgrade().unwrap();
            node.get_signal("connect").unwrap()
        };
        sig.trigger(serialize(&(peers_count as u32, is_dag_synced))).await;
    }

    /// Update the `outbound_peers` property with the outgoing connection slots addrs.
    /// Allows us to monitor the network state of our p2p node.
    /// Intermediate states append ` (status)` to the addr.
    async fn relay_outbound_slots(dnet_sub: Subscription<DnetEvent>, prop: PropertyPtr) {
        loop {
            let event = dnet_sub.receive().await;
            let (slot, kind, addr) = match event {
                DnetEvent::OutboundSlotConnected(info) => {
                    (info.slot, "connected", Some(info.addr.to_string()))
                }
                DnetEvent::OutboundSlotConnecting(info) => {
                    (info.slot, "connecting", Some(info.addr.to_string()))
                }
                DnetEvent::OutboundSlotDisconnected(info) => (info.slot, "disconnected", None),
                DnetEvent::OutboundSlotSleeping(info) => (info.slot, "sleeping", None),
                _ => continue,
            };

            let mut atom = PropertyAtomicGuard::none();
            let idx = slot as usize;
            assert!(idx < prop.get_len());
            match addr {
                Some(addr) if kind == "connected" => {
                    prop.set_str(&mut atom, Role::Internal, idx, addr).unwrap();
                }
                Some(addr) => {
                    let val = format!("{addr} ({kind})");
                    prop.set_str(&mut atom, Role::Internal, idx, val).unwrap();
                }
                None => prop.set_null(&mut atom, Role::Internal, idx).unwrap(),
            }
        }
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

            // Route the message. An already-decrypted (plaintext) message names a
            // channel we hold directly: encrypted channels arrive as base58
            // ciphertext, so a channel key we recognise is plaintext by definition
            // and is accepted as-is. Anything else must decrypt as a channel or DM;
            // undecryptable traffic (base58 garbage in neither map) is silently dropped.
            let mut privmsg = privmsg;
            // Is this a plaintext channel?
            let is_plaintext = self.channels.read().await.contains_key(&privmsg.channel);
            if !is_plaintext && !self.try_decrypt(&mut privmsg, &self.nick.get()).await {
                continue;
            }

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

            // Workaround for the chatview hack. This nick is off limits!
            let mut nick = privmsg.nick;
            if nick == "NOTICE" {
                nick = "noticer".to_string();
            }

            self.notify_recv(privmsg.channel, timest, msg_id, nick, privmsg.msg).await;
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
        assert!(
            channel.starts_with('#') || channel.starts_with('@'),
            "notify_recv channel must be a \"#name\" channel or \"@name\" DM, got: {channel}"
        );

        let mut arg_data = vec![];
        channel.encode(&mut arg_data).unwrap();
        timestamp.encode(&mut arg_data).unwrap();
        id.encode(&mut arg_data).unwrap();
        nick.encode(&mut arg_data).unwrap();
        msg.encode(&mut arg_data).unwrap();

        let sig = {
            let node = self.node.upgrade().unwrap();
            node.get_signal("recv").unwrap()
        };
        sig.trigger(arg_data).await;
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

        // DM layers use the "@name" UI id; strip it to the bare contact key and
        // require the contact to exist, else refuse to broadcast.
        if let Some(bare) = msg.channel.strip_prefix('@').map(str::to_string) {
            msg.channel = bare;
            if self.try_encrypt_dm(&mut msg).await.is_err() {
                e!("Refusing to send DM to unknown contact");
                return;
            }
        } else {
            assert!(msg.channel.starts_with('#'), "channel name must start with #");
            self.try_encrypt_channel(&mut msg).await;
        }
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

    /// Load channels from the app database and populate encryption keys
    pub async fn load_channels_from_db(&self) {
        let mut channels = self.channels.write().await;
        channels.clear();

        let joined: HashSet<String> = read_joined_channels().into_iter().collect();

        for ui_channel in self.app_db.channels().await.unwrap() {
            let full_name = format!("#{}", ui_channel.name);

            if !joined.contains(&full_name) {
                continue
            }

            // Convert to IrcChannel with encryption
            let mut irc_channel =
                IrcChannel { topic: String::new(), nicks: HashSet::new(), saltbox: None };

            if let Some(secret) = ui_channel.secret {
                // Convert secret array to SecretKey first, then derive PublicKey
                let secret_key = SecretKey::from_bytes(secret);
                let public = secret_key.public_key();
                let saltbox = ChaChaBox::new(&public, &secret_key);

                // The secret in base58 for debugging
                //let secret_b58 = bs58::encode(secret).into_string();
                irc_channel.saltbox = Some(Arc::new(saltbox));
            }

            let is_encrypted = irc_channel.saltbox.is_some();
            channels.insert(full_name, irc_channel);
            i!("Loaded channel: #{} (encrypted: {})", ui_channel.name, is_encrypted);
        }
    }

    /// Load contacts from the app database and build their encryption boxes.
    pub async fn load_contacts_from_db(&self) {
        let mut contacts = self.contacts.write().await;
        contacts.clear();

        let joined: HashSet<String> = read_joined_channels().into_iter().collect();

        for contact in self.app_db.contacts().await.unwrap() {
            if !joined.contains(&format!("@{}", contact.name)) {
                continue
            }

            let their_public = PublicKey::from(contact.public);
            let saltbox = Arc::new(ChaChaBox::new(&their_public, &self.dm_secret));
            let self_saltbox =
                Arc::new(ChaChaBox::new(&self.dm_secret.public_key(), &self.dm_secret));

            contacts.insert(contact.name.clone(), IrcContact { saltbox, self_saltbox });
            i!("Loaded contact: {}", contact.name);
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
            e!("DarkIrc destroyed before rescan completed");
            return false
        };

        // Decode channel name from method data
        let mut cur = std::io::Cursor::new(&method_call.data);
        let Ok(channel) = String::decode(&mut cur) else {
            e!("Rescan method called with invalid channel data");
            return false
        };

        self_.load_channels_from_db().await;
        self_.load_contacts_from_db().await;

        let task = self_.ex.clone().spawn(self_.clone().rescan_channel_history(channel));
        self_.tasks.lock().push(task);

        true
    }

    /// Apply the transport-specific P2P configuration (profiles, seeds,
    /// active profiles) using the defaults from darkirc_config.toml
    fn apply_transport_settings(settings: &mut NetSettings, transport: &str) {
        settings.seeds.clear();
        settings.profiles.clear();
        settings.active_profiles.clear();

        match transport {
            "tor" => {
                i!("Setup P2P network [tor]");
                let mut tor_profile = NetworkProfile::tor_default();
                tor_profile.outbound_connect_timeout = 60;
                settings.profiles.insert("tor".to_string(), tor_profile);
                settings.outbound_peer_discovery_cooloff_time = 60;

                settings.seeds.push(
                    url::Url::parse(
                        "tor://wgxxaifz5gv4iggcflyl67lgmsihffs6bbwobqah4np52t3y3olrnpid.onion:9601",
                    )
                    .unwrap(),
                );
                settings.seeds.push(
                    url::Url::parse(
                        "tor://inx5s3pdzddvgb5ii3oydutmbvw6fvor3oqu65wtxl3pyevtvrdn4had.onion:9601",
                    )
                    .unwrap(),
                );
                settings.active_profiles.push("tor".to_string());
            }
            "tcp" => {
                i!("Setup P2P network [clearnet]");
                let mut profile = NetworkProfile::default();
                profile.outbound_connect_timeout = 40;
                profile.channel_handshake_timeout = 30;
                settings.profiles.insert("tcp+tls".to_string(), profile);

                settings.seeds.push(url::Url::parse("tcp+tls://lilith0.dark.fi:9600").unwrap());
                settings.seeds.push(url::Url::parse("tcp+tls://lilith1.dark.fi:9600").unwrap());
                settings.active_profiles.push("tcp+tls".to_string());
            }
            unhandled => panic!("Unhandled net.transport value: {unhandled}"),
        }
    }

    /// `net.transport` was switched, reconfigure the P2P network and restart it
    async fn handle_transport_change(&self, transport: String) {
        i!("Transport changed to {transport}, restarting P2P network");

        let was_started = self.chat_is_enabled.get();
        if was_started {
            self.p2p.clone().stop().await;
        }

        let settings_lock = self.p2p.settings();
        let mut settings = settings_lock.write().await;
        Self::apply_transport_settings(&mut settings, &transport);
        drop(settings);

        if was_started {
            while let Err(err) = self.p2p.clone().start().await {
                e!("Failed to start P2P network: {err}!");
                e!("Retrying in {P2P_RETRY_TIME} secs");
                sleep(P2P_RETRY_TIME).await;
            }

            let peers_count = self.p2p.peers_count();
            self.notify_connect(peers_count, self.event_graph.is_synced()).await;
        }

        i!("P2P transport restart completed");
    }

    /// `chat.is_enabled` was switched on
    async fn handle_start(&self) {
        i!("Manual P2P start triggered");

        while let Err(err) = self.p2p.clone().start().await {
            e!("Failed to start P2P network: {err}!");
            e!("Retrying in {P2P_RETRY_TIME} secs");
            sleep(P2P_RETRY_TIME).await;
        }

        let peers_count = self.p2p.peers_count();
        self.notify_connect(peers_count, self.event_graph.is_synced()).await;

        i!("P2P start completed");
    }

    /// `chat.is_enabled` was switched off
    async fn handle_stop(&self) {
        i!("Manual P2P stop triggered");
        self.p2p.clone().stop().await;

        // Stopped outbound slots emit no dnet events, so clear the property here
        let node = self.node.upgrade().unwrap();
        let prop = node.get_property("outbound_peers").unwrap();
        let mut atom = PropertyAtomicGuard::none();
        for idx in 0..prop.get_len() {
            prop.set_null(&mut atom, Role::Internal, idx).unwrap();
        }

        self.notify_connect(0, self.event_graph.is_synced()).await;
        i!("P2P stop completed");
    }

    async fn set_outbound_connections(&self, count: usize) {
        let p2p_settings = self.p2p.settings();

        if p2p_settings.read().await.outbound_connections == count {
            return;
        }

        p2p_settings.write().await.outbound_connections = count;
        self.p2p.clone().reload().await;
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

        let chat_is_enabled = self.chat_is_enabled.clone();
        let chat_is_enabled_sub = chat_is_enabled.prop().subscribe_modify();
        let me2 = me.clone();
        let setting_task = ex.spawn(async move {
            if chat_is_enabled.get() {
                let Some(self_) = me2.upgrade() else { return };
                self_.handle_start().await;
            }

            while let Ok(_) = chat_is_enabled_sub.receive().await {
                let Some(self_) = me2.upgrade() else { break };

                if chat_is_enabled.get() {
                    self_.handle_start().await;
                } else {
                    self_.handle_stop().await;
                }
            }
        });

        let net_transport = self.net_transport.clone();
        let net_transport_sub = net_transport.prop().subscribe_modify();
        let me2 = me.clone();
        let transport_task = ex.spawn(async move {
            while let Ok(_) = net_transport_sub.receive().await {
                let Some(self_) = me2.upgrade() else { break };

                self_.handle_transport_change(net_transport.get()).await;
            }
        });

        let rescan_method_sub = node.subscribe_method_call("rescan").unwrap();
        let me2 = me.clone();
        let rescan_method_task =
            ex.spawn(async move { while Self::process_rescan(&me2, &rescan_method_sub).await {} });

        let mut on_modify = OnModify::new(ex.clone(), self.node.clone(), me.clone());
        async fn save_nick(self_: Arc<DarkIrc>, _batch: BatchGuardPtr) {
            if let Err(err) = self_.app_db.nick_set(&self_.nick.get()).await {
                e!("Failed persisting nick to app db: {err}");
            }
        }
        on_modify.when_change(self.nick.prop(), save_nick);

        let ev_sub = self.event_graph.event_subscribe().await;
        let ev_task = ex.spawn(self.clone().relay_events(ev_sub));

        // Sync the DAG / check sync status
        let channel_sub = self.p2p.hosts().subscribe_channel().await;
        let dag_task = ex.spawn(self.clone().dag_sync(channel_sub));

        let window_node = sg_root.lookup_node("/window").unwrap();

        let (start_slot, start_recv) = Slot::new("darkirc_start");
        window_node.register("start", start_slot).unwrap();

        let me2 = Arc::downgrade(&self);
        let start_task = ex.spawn(async move {
            while let Ok(_) = start_recv.recv().await {
                let Some(self_) = me2.upgrade() else { break };

                self_.set_outbound_connections(P2P_OUTBOUND_ACTIVE).await;
            }
        });

        let (stop_slot, stop_recv) = Slot::new("darkirc_stop");
        window_node.register("stop", stop_slot).unwrap();

        let me2 = Arc::downgrade(&self);
        let stop_task = ex.spawn(async move {
            while let Ok(_) = stop_recv.recv().await {
                let Some(self_) = me2.upgrade() else { break };

                self_.set_outbound_connections(P2P_OUTBOUND_SLEEP).await;
            }
        });

        let (screen_changed_slot, screen_changed_recv) = Slot::new("darkirc_screen_changed");
        window_node.register("screen_changed", screen_changed_slot).unwrap();

        let me2 = Arc::downgrade(&self);
        let screen_changed_task = ex.spawn(async move {
            while let Ok(data) = screen_changed_recv.recv().await {
                let Some(self_) = me2.upgrade() else { break };

                let mut cursor = Cursor::new(&data);
                let Ok(screen_on) = bool::decode(&mut cursor) else { continue };

                if screen_on {
                    self_.set_outbound_connections(P2P_OUTBOUND_ACTIVE).await;
                } else {
                    self_.set_outbound_connections(P2P_OUTBOUND_SLEEP).await;
                }
            }
        });

        let mut tasks = vec![
            send_method_task,
            setting_task,
            transport_task,
            rescan_method_task,
            ev_task,
            dag_task,
            start_task,
            stop_task,
            screen_changed_task,
        ];

        if DNET_ENABLED {
            let dnet_sub = self.p2p.dnet_subscribe().await;
            let node = self.node.upgrade().unwrap();
            let prop = node.get_property("outbound_peers").unwrap();
            let dnet_task = ex.spawn(Self::relay_outbound_slots(dnet_sub, prop));
            tasks.push(dnet_task);
        }

        tasks.append(&mut on_modify.tasks);
        *self.tasks.lock() = tasks;
    }

    /// Encrypt a channel `Privmsg` in place if the channel has a shared key.
    /// Open channels with no key are left plaintext.
    pub async fn try_encrypt_channel(&self, privmsg: &mut Privmsg) {
        let guard = self.channels.read().await;
        let Some((name, channel)) = guard.get_key_value(&privmsg.channel) else {
            return;
        };
        let Some(saltbox) = &channel.saltbox else {
            return;
        };
        privmsg.channel = saltbox::encrypt(saltbox, &[0x00; MAX_NICK_LEN]);
        privmsg.nick = saltbox::encrypt(saltbox, &pad(&privmsg.nick));
        privmsg.msg = saltbox::encrypt(saltbox, privmsg.msg.as_bytes());
        d!("Successfully encrypted message for {name}");
    }

    /// Encrypt a DM `Privmsg` in place for the contact named by `privmsg.channel`
    /// (the bare key, with no leading "@"). Fails if the contact is unknown so
    /// the caller can refuse to broadcast a message no one could decrypt.
    pub async fn try_encrypt_dm(&self, privmsg: &mut Privmsg) -> Result<()> {
        let guard = self.contacts.read().await;
        let Some((name, contact)) = guard.get_key_value(&privmsg.channel) else {
            return Err(Error::ContactNotFound);
        };
        privmsg.channel = saltbox::encrypt(&contact.saltbox, &[0x00; MAX_NICK_LEN]);
        privmsg.nick = saltbox::encrypt(&contact.self_saltbox, &[0x00; MAX_NICK_LEN]);
        privmsg.msg = saltbox::encrypt(&contact.saltbox, privmsg.msg.as_bytes());
        d!("Successfully encrypted DM for {name}");
        Ok(())
    }

    /// Try decrypting a `Privmsg` as a channel message in place. Returns true on
    /// success. Plaintext messages for a known keyless channel are accepted
    /// as-is; everything else returns false.
    pub async fn try_decrypt_channel(&self, privmsg: &mut Privmsg) -> bool {
        let Ok(channel_ciphertext) = bs58::decode(&privmsg.channel).into_vec() else {
            // Not encrypted: accept only if it names a channel we hold.
            return self.channels.read().await.contains_key(&privmsg.channel);
        };
        let Ok(nick_ciphertext) = bs58::decode(&privmsg.nick).into_vec() else { return false };
        let Ok(msg_ciphertext) = bs58::decode(&privmsg.msg).into_vec() else { return false };

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
            return true
        }

        false
    }

    /// Try decrypting a `Privmsg` as a DM in place. Returns true on success.
    pub async fn try_decrypt_contact(&self, privmsg: &mut Privmsg, self_nickname: &str) -> bool {
        let Ok(channel_ciphertext) = bs58::decode(&privmsg.channel).into_vec() else {
            return false
        };
        let Ok(nick_ciphertext) = bs58::decode(&privmsg.nick).into_vec() else { return false };
        let Ok(msg_ciphertext) = bs58::decode(&privmsg.msg).into_vec() else { return false };

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

            privmsg.channel = format!("@{}", name);
            privmsg.nick = nick;
            privmsg.msg = String::from_utf8_lossy(&msg_dec).into();
            return true
        }

        false
    }

    /// Try decrypting a given potentially encrypted `Privmsg` object as a channel
    /// and then as a DM. Returns true if either succeeded.
    pub async fn try_decrypt(&self, privmsg: &mut Privmsg, self_nickname: &str) -> bool {
        self.try_decrypt_channel(privmsg).await ||
            self.try_decrypt_contact(privmsg, self_nickname).await
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
