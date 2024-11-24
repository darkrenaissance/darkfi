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

use std::{
    sync::{Arc, Mutex as SyncMutex},
    time::UNIX_EPOCH,
};

use darkfi::{
    event_graph::{
        self,
        proto::{EventPut, ProtocolEventGraph},
        EventGraph, EventGraphPtr,
    },
    net::{session::SESSION_DEFAULT, settings::Settings as NetSettings, P2p, P2pPtr},
    system::{sleep, Subscription},
    Error,
};
use darkfi_serial::{
    async_trait, deserialize_async, serialize_async, AsyncEncodable, Encodable, SerialDecodable,
    SerialEncodable,
};
use sled_overlay::sled;

use crate::{
    prop::{PropertyBool, PropertyFloat32, PropertyStr, Role},
    scene::{SceneNodePtr, Slot},
    ui::chatview::MessageId,
    ExecutorPtr,
};

#[cfg(target_os = "android")]
const EVGRDB_PATH: &str = "/data/data/darkfi.darkwallet/evgr/";
#[cfg(not(target_os = "android"))]
const EVGRDB_PATH: &str = "~/.local/darkfi/darkwallet/evgr/";

const CHANNEL: &str = "#random";

/// Due to drift between different machine's clocks, if the message timestamp is recent
/// then we will just correct it to the current time so messages appear sequential in the UI.
const RECENT_TIME_DIST: u64 = 10_000;

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
        timest.encode(&mut hasher).unwrap();
        self.channel.encode(&mut hasher).unwrap();
        self.nick.encode(&mut hasher).unwrap();
        self.msg.encode(&mut hasher).unwrap();
        MessageId(hasher.finalize().into())
    }
}

pub type DarkIrcBackendPtr = Arc<DarkIrcBackend>;

pub struct DarkIrcBackend {
    ex: ExecutorPtr,
    p2p: P2pPtr,
    event_graph: EventGraphPtr,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    db: sled::Db,

    chatview_node: SceneNodePtr,
    sendbtn_node: SceneNodePtr,
    editbox_node: SceneNodePtr,
    editbox_text: PropertyStr,
    chatview_scroll: PropertyFloat32,
    upgrade_popup_is_visible: PropertyBool,

    seen_msgs: SyncMutex<Vec<MessageId>>,
}

impl DarkIrcBackend {
    pub async fn new(sg_root: SceneNodePtr, ex: ExecutorPtr) -> darkfi::Result<Arc<Self>> {
        let chatview_node = sg_root.clone().lookup_node("/window/view/chatty").unwrap();
        let sendbtn_node = sg_root.clone().lookup_node("/window/view/send_btn").unwrap();

        let editbox_node = sg_root.clone().lookup_node("/window/view/editz").unwrap();
        let editbox_text = PropertyStr::wrap(&editbox_node, Role::App, "text", 0).unwrap();

        let chatview_scroll =
            PropertyFloat32::wrap(&chatview_node, Role::Internal, "scroll", 0).unwrap();

        let upgrade_popup_node = sg_root.clone().lookup_node("/window/view/upgrade_popup").unwrap();
        let upgrade_popup_is_visible =
            PropertyBool::wrap(&upgrade_popup_node, Role::App, "is_visible", 0).unwrap();

        info!(target: "darkirc", "Starting DarkIRC backend");
        let db = sled::open(EVGRDB_PATH)?;

        let mut p2p_settings: NetSettings = Default::default();
        p2p_settings.app_version = semver::Version::parse("0.5.0").unwrap();
        p2p_settings.seeds.push(url::Url::parse("tcp+tls://lilith1.dark.fi:5262").unwrap());

        let p2p = P2p::new(p2p_settings, ex.clone()).await?;

        let event_graph = EventGraph::new(
            p2p.clone(),
            db.clone(),
            std::path::PathBuf::new(),
            false,
            "darkirc_dag",
            1,
            ex.clone(),
        )
        .await?;

        Ok(Arc::new(Self {
            ex,
            p2p,
            event_graph,
            tasks: SyncMutex::new(vec![]),
            db,

            chatview_node,
            sendbtn_node,
            editbox_node,
            editbox_text,
            chatview_scroll,
            upgrade_popup_is_visible,

            seen_msgs: SyncMutex::new(vec![]),
        }))
    }

    pub async fn start(self: Arc<Self>, ex: ExecutorPtr) -> darkfi::Result<()> {
        //self.prune_task.lock().unwrap() = Some(event_graph.prune_task.get().unwrap());

        info!(target: "darkirc", "Registering EventGraph P2P protocol");
        let event_graph_ = Arc::clone(&self.event_graph);
        let registry = self.p2p.protocol_registry();
        registry
            .register(SESSION_DEFAULT, move |channel, _| {
                let event_graph_ = event_graph_.clone();
                async move { ProtocolEventGraph::init(event_graph_, channel).await.unwrap() }
            })
            .await;

        let ev_sub = self.event_graph.event_pub.clone().subscribe().await;
        let ev_task = self.ex.spawn(self.clone().relay_events(ev_sub));

        info!(target: "darkirc", "Starting P2P network");
        self.p2p.clone().start().await?;

        // Connect the UI send up

        let (slot, recvr) = Slot::new("send_button_clicked");
        self.sendbtn_node.register("click", slot).unwrap();
        let me = Arc::downgrade(&self);
        let send_task = ex.spawn(async move {
            while let Some(self_) = me.upgrade() {
                let Ok(_) = recvr.recv().await else {
                    error!(target: "ui::win", "Button click recvr closed");
                    break
                };
                self_.handle_send().await;
            }
        });

        let (slot, recvr) = Slot::new("enter_pressed");
        self.editbox_node.register("enter_pressed", slot).unwrap();
        let me = Arc::downgrade(&self);
        let enter_task = ex.spawn(async move {
            while let Some(self_) = me.upgrade() {
                let Ok(_) = recvr.recv().await else {
                    error!(target: "ui::win", "EditBox enter_pressed recvr closed");
                    break
                };
                self_.handle_send().await;
            }
        });

        {
            let mut tasks = self.tasks.lock().unwrap();
            assert!(tasks.is_empty());
            *tasks = vec![send_task, enter_task];
        }

        // Sync the DAG

        info!(target: "darkirc", "Waiting for some P2P connections...");
        sleep(5).await;

        // We'll attempt to sync {sync_attempts} times
        let sync_attempts = 4;
        for i in 1..=sync_attempts {
            info!(target: "darkirc", "Syncing event DAG (attempt #{})", i);
            match self.event_graph.dag_sync().await {
                Ok(()) => break,
                Err(e) => {
                    if i == sync_attempts {
                        error!("Failed syncing DAG. Exiting.");
                        self.p2p.stop().await;
                        return Err(Error::DagSyncFailed)
                    } else {
                        // TODO: Maybe at this point we should prune or something?
                        // TODO: Or maybe just tell the user to delete the DAG from FS.
                        error!("Failed syncing DAG ({}), retrying in {}s...", e, 4);
                        sleep(4).await;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn stop(&self) {
        info!(target: "darkirc", "Stopping DarkIRC backend");

        info!(target: "darkirc", "Stopping P2P network");
        self.p2p.stop().await;

        info!(target: "darkirc", "Stopping event graph prune task");
        let prune_task = self.event_graph.prune_task.get().unwrap();
        prune_task.stop().await;

        info!(target: "darkirc", "Flushing event graph sled database...");
        let Ok(flushed_bytes) = self.db.flush_async().await else {
            error!(target: "darkirc", "Flushing event graph db failed");
            return
        };
        info!(target: "darkirc", "Flushed {} bytes", flushed_bytes);
        info!(target: "darkirc", "Shut down backend successfully");
    }

    async fn relay_events(self: Arc<Self>, ev_sub: Subscription<event_graph::Event>) {
        loop {
            let ev = ev_sub.receive().await;

            // Try to deserialize the `Event`'s content into a `Privmsg`
            let privmsg: Privmsg = match deserialize_async(ev.content()).await {
                Ok(v) => v,
                Err(e) => {
                    error!("[IRC CLIENT] Failed deserializing incoming Privmsg event: {}", e);
                    continue
                }
            };

            if privmsg.channel != CHANNEL {
                continue
            }

            info!(target: "darkirc", "ev_id={:?}", ev.id());
            info!(target: "darkirc", "ev: {:?}", ev);
            info!(target: "darkirc", "privmsg: {:?}", privmsg);
            info!(target: "darkirc", "");

            let timest = ev.timestamp;
            // This is a hack to make messages appear sequentially in the UI
            let mut adj_timest = timest;
            let now_timest = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
            if timest.abs_diff(now_timest) < RECENT_TIME_DIST {
                debug!(target: "darkirc", "Applied timestamp correction: <{timest}> => <{now_timest}>");
                adj_timest = now_timest;
            }

            let msg_id = privmsg.msg_id(timest);
            {
                let mut seen = self.seen_msgs.lock().unwrap();
                if seen.contains(&msg_id) {
                    warn!(target: "darkirc", "Skipping duplicate seen message: {msg_id}");
                    continue
                }
                seen.push(msg_id.clone());
            }

            let mut arg_data = vec![];
            ev.timestamp.encode(&mut arg_data).unwrap();
            ev.id().as_bytes().encode(&mut arg_data).unwrap();
            privmsg.nick.encode(&mut arg_data).unwrap();
            privmsg.msg.encode(&mut arg_data).unwrap();

            self.chatview_node.call_method("insert_line", arg_data).await.unwrap();
        }
    }

    async fn handle_send(&self) {
        // Get text from editbox
        let text = self.editbox_text.get();
        if text.is_empty() {
            return
        }
        // Clear editbox
        self.editbox_text.set("");
        self.chatview_scroll.set(0.);

        // Send text to channel
        let timest = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
        debug!(target: "darkirc", "Sending privmsg: <{timest}> {text}");
        let msg = Privmsg::new(CHANNEL.to_string(), "anon".to_string(), text);

        let mut arg_data = vec![];
        timest.encode_async(&mut arg_data).await.unwrap();
        msg.msg_id(timest).encode_async(&mut arg_data).await.unwrap();
        msg.nick.encode_async(&mut arg_data).await.unwrap();
        msg.msg.encode_async(&mut arg_data).await.unwrap();

        self.chatview_node.call_method("insert_unconf_line", arg_data).await.unwrap();

        // Broadcast the msg

        let evgr = self.event_graph.clone();
        let event = event_graph::Event::new(serialize_async(&msg).await, &evgr).await;
        if let Err(e) = evgr.dag_insert(&[event.clone()]).await {
            error!(target: "darkirc", "Failed inserting new event to DAG: {}", e);
        }

        self.p2p.broadcast(&EventPut(event)).await;
    }
}
