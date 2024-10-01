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

use async_channel::{Receiver, Sender};
use async_lock::Mutex as AsyncMutex;
use darkfi::{
    event_graph::{self},
    net::transport::{Dialer, PtStream},
    system::{sleep, ExecutorPtr},
    util::path::expand_path,
    Error, Result,
};
use darkfi_serial::{
    async_trait, deserialize_async_partial, serialize_async, AsyncDecodable, AsyncEncodable,
    Encodable, SerialDecodable, SerialEncodable,
};
use evgrd::{
    FetchEventsMessage, LocalEventGraph, LocalEventGraphPtr, VersionMessage, MSG_EVENT,
    MSG_FETCHEVENTS, MSG_SENDEVENT,
};
use futures::{select, AsyncWriteExt, FutureExt};
use log::{error, info};
use sled_overlay::sled;
use smol::{
    fs,
    io::{ReadHalf, WriteHalf},
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex as SyncMutex, Weak,
    },
    time::UNIX_EPOCH,
};
use url::Url;

use crate::{
    prop::{PropertyBool, PropertyFloat32, PropertyStr, Role},
    scene::{SceneNodePtr, Slot},
    ui::chatview::MessageId,
};

#[cfg(target_os = "android")]
const EVGRDB_PATH: &str = "/data/data/darkfi.darkwallet/evgr/";
#[cfg(not(target_os = "android"))]
const EVGRDB_PATH: &str = "~/.local/darkfi/darkwallet/evgr/";

//const ENDPOINT: &str = "tcp://agorism.dev:25588";
const ENDPOINT: &str = "tor://obbc5rgtsqtscnph7yxrbsgsm5axbppfn552yr5lrrd2ocgkdcsjcnyd.onion:25589";
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

pub type LocalDarkIRCPtr = Arc<LocalDarkIRC>;

pub struct LocalDarkIRC {
    stream: AsyncMutex<Option<Box<dyn PtStream>>>,

    evgr: LocalEventGraphPtr,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    send_sender: Sender<(u64, Privmsg)>,
    send_recvr: Receiver<(u64, Privmsg)>,

    chatview_node: SceneNodePtr,
    sendbtn_node: SceneNodePtr,
    editbox_node: SceneNodePtr,
    editbox_text: PropertyStr,
    chatview_scroll: PropertyFloat32,
    upgrade_popup_is_visible: PropertyBool,

    seen_msgs: SyncMutex<Vec<MessageId>>,
}

impl LocalDarkIRC {
    pub async fn new(sg_root: SceneNodePtr, ex: ExecutorPtr) -> Result<Arc<Self>> {
        let chatview_node = sg_root.clone().lookup_node("/window/view/chatty").unwrap();
        let sendbtn_node = sg_root.clone().lookup_node("/window/view/send_btn").unwrap();

        let editbox_node = sg_root.clone().lookup_node("/window/view/editz").unwrap();
        let editbox_text = PropertyStr::wrap(&editbox_node, Role::App, "text", 0).unwrap();

        let chatview_scroll =
            PropertyFloat32::wrap(&chatview_node, Role::Internal, "scroll", 0).unwrap();

        let upgrade_popup_node = sg_root.clone().lookup_node("/window/view/upgrade_popup").unwrap();
        let upgrade_popup_is_visible =
            PropertyBool::wrap(&upgrade_popup_node, Role::App, "is_visible", 0).unwrap();

        info!(target: "darkirc", "Instantiating DarkIRC event DAG");
        let datastore = expand_path(EVGRDB_PATH)?;
        fs::create_dir_all(&datastore).await?;
        let sled_db = sled::open(datastore)?;

        let evgr = LocalEventGraph::new(sled_db.clone(), "darkirc_dag", 1, ex.clone()).await?;

        let (send_sender, send_recvr) = async_channel::unbounded();

        Ok(Arc::new(Self {
            stream: AsyncMutex::new(None),

            evgr,
            tasks: SyncMutex::new(vec![]),
            send_sender,
            send_recvr,

            chatview_node,
            sendbtn_node,
            editbox_node,
            editbox_text,
            chatview_scroll,
            upgrade_popup_is_visible,

            seen_msgs: SyncMutex::new(vec![]),
        }))
    }

    pub async fn start(self: Arc<Self>, ex: ExecutorPtr) -> Result<()> {
        debug!(target: "darkirc", "LocalDarkIRC::start()");

        let me = Arc::downgrade(&self);
        let mainloop_task = ex.spawn(Self::run_mainloop(me));

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

        let mut tasks = self.tasks.lock().unwrap();
        assert!(tasks.is_empty());
        *tasks = vec![mainloop_task, send_task, enter_task];

        Ok(())
    }

    async fn run_mainloop(me: Weak<Self>) {
        let mut send_queue: Vec<(u64, Privmsg)> = vec![];

        'reconnect: loop {
            loop {
                debug!(target: "darkirc", "Connecting to evgrd...");
                let Some(self_) = me.upgrade() else { return };
                while let Err(e) = self_.connect().await {
                    error!(target: "darkirc", "Unable to connect to evgrd backend: {e}");
                    sleep(2).await;
                }
                debug!(target: "darkirc", "Attempting version exchange...");
                let Err(e) = self_.version_exchange().await else { break };
                error!(target: "darkirc", "Version exchange with evgrd failed: {e}");
            }

            info!(target: "darkirc", "Connected to evgrd backend");

            let Some(self_) = me.upgrade() else { return };
            if !send_queue.is_empty() {
                info!(target: "darkirc", "Resending {} messages", send_queue.len());
            }
            while let Some((timest, privmsg)) = send_queue.pop() {
                if let Err(e) = self_.send_msg(timest, privmsg.clone()).await {
                    error!(target: "darkirc", "Send failed");
                    send_queue.push((timest, privmsg));
                    continue 'reconnect
                }
            }
            drop(self_);

            loop {
                let Some(self_) = me.upgrade() else { return };

                select! {
                    res = self_.receive_msg().fuse() => {
                        if let Err(e) = res {
                            error!(target: "darkirc", "Receive failed: {e}");
                            continue 'reconnect
                        };
                    }
                    res = self_.send_recvr.recv().fuse() => {
                        let (timest, privmsg) = res.unwrap();
                        info!(target: "darkirc", "Sending msg: {timest} {privmsg:?}");
                        if let Err(e) = self_.send_msg(timest, privmsg.clone()).await {
                            error!(target: "darkirc", "Send failed");
                            send_queue.push((timest, privmsg));
                            continue 'reconnect
                        }
                    }
                }
            }
        }
    }

    async fn connect(&self) -> Result<()> {
        let endpoint = Url::parse(ENDPOINT)?;

        let dialer = Dialer::new(endpoint.clone(), None).await?;
        let timeout = std::time::Duration::from_secs(60);

        let stream = dialer.dial(Some(timeout)).await?;
        info!(target: "darkirc", "Connected to the backend: {endpoint}");

        *self.stream.lock().await = Some(stream);

        Ok(())
    }

    async fn version_exchange(&self) -> Result<()> {
        let Some(stream) = &mut *self.stream.lock().await else { return Err(Error::ConnectFailed) };

        let version = VersionMessage::new();
        debug!(target: "darkirc", "Sending version: {version:?}");
        version.encode_async(stream).await?;
        stream.flush().await?;

        debug!(target: "darkirc", "Receiving version...");
        let server_version = VersionMessage::decode_async(stream).await?;
        info!(target: "darkirc", "Backend server version: {}", server_version.protocol_version);

        if server_version.protocol_version > evgrd::PROTOCOL_VERSION {
            self.upgrade_popup_is_visible.set(true);
        }

        let unref_tips = self.evgr.unreferenced_tips.read().await.clone();
        let fetchevs = FetchEventsMessage::new(unref_tips);
        MSG_FETCHEVENTS.encode_async(stream).await?;
        stream.flush().await?;
        fetchevs.encode_async(stream).await?;
        stream.flush().await?;

        Ok(())
    }

    async fn send_msg(&self, timestamp: u64, msg: Privmsg) -> Result<()> {
        let Some(stream) = &mut *self.stream.lock().await else { return Err(Error::ConnectFailed) };

        MSG_SENDEVENT.encode_async(stream).await?;
        stream.flush().await?;
        timestamp.encode_async(stream).await?;
        stream.flush().await?;

        let content: Vec<u8> = serialize_async(&msg).await;
        content.encode_async(stream).await?;
        stream.flush().await?;

        Ok(())
    }

    async fn receive_msg(&self) -> Result<()> {
        debug!(target: "darkirc", "Receiving message...");

        let Some(stream) = &mut *self.stream.lock().await else { return Err(Error::ConnectFailed) };

        let msg_type = u8::decode_async(stream).await?;
        debug!(target: "darkirc", "Received: {msg_type:?}");
        if msg_type != MSG_EVENT {
            error!(target: "darkirc", "Received invalid msg_type: {msg_type}");
            //return Err(Error::MalformedPacket)
            return Ok(())
        }

        let ev = event_graph::Event::decode_async(stream).await?;

        let privmsg: Privmsg = match deserialize_async_partial(ev.content()).await {
            Ok((v, _)) => v,
            Err(e) => {
                error!(target: "darkirc", "Failed deserializing incoming Privmsg event: {e}");
                return Ok(())
            }
        };

        let mut timest = ev.timestamp;
        if timest < 6047051717 {
            timest *= 1000;
        }
        debug!(target: "darkirc", "Recv privmsg: <{timest}> {privmsg:?}");

        let genesis_timestamp = self.evgr.current_genesis.read().await.clone().timestamp;
        let ev_id = ev.id();
        if self.evgr.dag.contains_key(ev_id.as_bytes()).unwrap() ||
            !ev.validate(&self.evgr.dag, genesis_timestamp, self.evgr.days_rotation, None)
                .await?
        {
            error!(target: "darkirc", "Event is invalid! {ev:?}");
            return Ok(())
        }

        self.evgr.dag_insert(&[ev.clone()]).await.unwrap();

        if privmsg.channel != CHANNEL {
            //debug!(target: "darkirc", "{} != {CHANNEL}", privmsg.channel);
            return Ok(())
        }

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
                return Ok(())
            }
            seen.push(msg_id.clone());
        }

        let mut arg_data = vec![];
        adj_timest.encode_async(&mut arg_data).await.unwrap();
        msg_id.encode_async(&mut arg_data).await.unwrap();
        privmsg.nick.encode_async(&mut arg_data).await.unwrap();
        privmsg.msg.encode_async(&mut arg_data).await.unwrap();

        self.chatview_node.call_method("insert_line", arg_data).await.unwrap();

        Ok(())
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

        self.send_sender.send((timest, msg)).await.unwrap();

        self.chatview_node.call_method("insert_unconf_line", arg_data).await.unwrap();
    }
}
