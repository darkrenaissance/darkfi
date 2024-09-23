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

use async_lock::Mutex as AsyncMutex;
use darkfi::{
    event_graph::{self},
    net::transport::{Dialer, PtStream},
    system::ExecutorPtr,
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
    prop::{PropertyStr, Role},
    scene::{SceneNodePtr, Slot},
};

#[cfg(target_os = "android")]
const EVGRDB_PATH: &str = "/data/data/darkfi.darkwallet/evgr/";
#[cfg(target_os = "linux")]
const EVGRDB_PATH: &str = "~/.local/darkfi/darkwallet/evgr/";

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
}

pub type LocalDarkIRCPtr = Arc<LocalDarkIRC>;

pub struct LocalDarkIRC {
    is_connected: AtomicBool,
    /// The reading half of the transport stream
    reader: AsyncMutex<Option<ReadHalf<Box<dyn PtStream>>>>,
    /// The writing half of the transport stream
    writer: AsyncMutex<Option<WriteHalf<Box<dyn PtStream>>>>,

    evgr: LocalEventGraphPtr,
    tasks: SyncMutex<Vec<smol::Task<()>>>,

    chatview_node: SceneNodePtr,
    sendbtn_node: SceneNodePtr,
    editbox_text: PropertyStr,
}

impl LocalDarkIRC {
    pub async fn new(sg_root: SceneNodePtr, ex: ExecutorPtr) -> Result<Arc<Self>> {
        let chatview_node = sg_root.clone().lookup_node("/window/view/chatty").unwrap();
        let sendbtn_node = sg_root.clone().lookup_node("/window/view/send_btn").unwrap();

        let editbox_node = sg_root.lookup_node("/window/view/editz").unwrap();
        let editbox_text = PropertyStr::wrap(&editbox_node, Role::App, "text", 0).unwrap();

        info!(target: "darkirc", "Instantiating DarkIRC event DAG");
        let datastore = expand_path(EVGRDB_PATH)?;
        fs::create_dir_all(&datastore).await?;
        let sled_db = sled::open(datastore)?;

        let evgr = LocalEventGraph::new(sled_db.clone(), "darkirc_dag", 1, ex.clone()).await?;

        Ok(Arc::new(Self {
            is_connected: AtomicBool::new(false),
            reader: AsyncMutex::new(None),
            writer: AsyncMutex::new(None),

            evgr,
            tasks: SyncMutex::new(vec![]),

            chatview_node,
            sendbtn_node,
            editbox_text,
        }))
    }

    async fn reconnect(&self) -> Result<()> {
        let endpoint = "tcp://127.0.0.1:5588";
        let endpoint = Url::parse(endpoint)?;

        let dialer = Dialer::new(endpoint.clone(), None).await?;
        let timeout = std::time::Duration::from_secs(60);

        let stream = dialer.dial(Some(timeout)).await?;
        info!(target: "darkirc", "Connected to the backend: {endpoint}");

        let (reader, writer) = smol::io::split(stream);
        *self.writer.lock().await = Some(writer);
        *self.reader.lock().await = Some(reader);

        Ok(())
    }

    pub async fn start(self: Arc<Self>, ex: ExecutorPtr) -> Result<()> {
        debug!(target: "darkirc", "LocalDarkIRC::start()");

        //self.reconnect().await?;
        self.version_exchange().await?;

        let me = Arc::downgrade(&self);
        let recv_task = ex.spawn(async move {
            while let Some(self_) = me.upgrade() {
                self_.receive_msg().await.unwrap();
            }
            error!(target: "darkirc", "Closing DarkIRC receive loop");
        });

        let (slot_click, click_recvr) = Slot::new("send_button_clicked");
        self.sendbtn_node.register("click", slot_click).unwrap();

        let me = Arc::downgrade(&self);
        let send_task = ex.spawn(async move {
            while let Some(self_) = me.upgrade() {
                let Ok(_) = click_recvr.recv().await else {
                    error!(target: "ui::win", "Button click recvr closed");
                    break
                };
                self_.handle_send().await;
            }
        });

        let mut tasks = self.tasks.lock().unwrap();
        assert!(tasks.is_empty());
        *tasks = vec![recv_task, send_task];

        Ok(())
    }

    async fn version_exchange(&self) -> Result<()> {
        if !self.is_connected.load(Ordering::Relaxed) {
            self.reconnect().await?;
        }

        let mut writer = self.writer.lock().await;
        let mut reader = self.reader.lock().await;
        let writer = writer.as_mut().unwrap();
        let reader = reader.as_mut().unwrap();

        let version = VersionMessage::new();
        version.encode_async(writer).await?;

        let server_version = VersionMessage::decode_async(reader).await?;
        info!(target: "darkirc", "Backend server version: {}", server_version.protocol_version);

        let unref_tips = self.evgr.unreferenced_tips.read().await.clone();
        let fetchevs = FetchEventsMessage::new(unref_tips);
        MSG_FETCHEVENTS.encode_async(writer).await?;
        fetchevs.encode_async(writer).await?;

        self.is_connected.store(true, Ordering::Relaxed);

        Ok(())
    }

    async fn send_msg(&self, timestamp: u64, msg: Privmsg) -> Result<()> {
        if !self.is_connected.load(Ordering::Relaxed) {
            debug!(target: "darkirc", "send_msg: not connected, reconnecting...");
            self.reconnect().await?;
        }

        let mut writer = self.writer.lock().await;
        let writer = writer.as_mut().unwrap();

        MSG_SENDEVENT.encode_async(writer).await?;
        timestamp.encode_async(writer).await?;

        let content: Vec<u8> = serialize_async(&msg).await;
        content.encode_async(writer).await?;

        Ok(())
    }

    async fn receive_msg(&self) -> Result<()> {
        if !self.is_connected.load(Ordering::Relaxed) {
            self.reconnect().await?;
        }

        debug!(target: "darkirc", "Receiving message...");

        let mut reader = self.reader.lock().await;
        let reader = reader.as_mut().unwrap();

        let msg_type = u8::decode_async(reader).await?;
        debug!(target: "darkirc", "Received: {msg_type:?}");
        if msg_type != MSG_EVENT {
            error!(target: "darkirc", "Received invalid msg_type: {msg_type}");
            //return Err(Error::MalformedPacket)
            return Ok(())
        }

        let ev = event_graph::Event::decode_async(reader).await?;

        let genesis_timestamp = self.evgr.current_genesis.read().await.clone().timestamp;
        let ev_id = ev.id();
        if self.evgr.dag.contains_key(ev_id.as_bytes()).unwrap() ||
            !ev.validate(&self.evgr.dag, genesis_timestamp, self.evgr.days_rotation, None)
                .await?
        {
            error!(target: "darkirc", "Event is invalid! {ev:?}");
            return Ok(())
        }

        debug!(target: "darkirc", "got {ev:?}");
        self.evgr.dag_insert(&[ev.clone()]).await.unwrap();

        let privmsg: Privmsg = match deserialize_async_partial(ev.content()).await {
            Ok((v, _)) => v,
            Err(e) => {
                error!(target: "darkirc", "Failed deserializing incoming Privmsg event: {e}");
                return Ok(())
            }
        };

        debug!(target: "darkirc", "privmsg: {privmsg:?}");
        let mut timest = ev.timestamp;
        if timest < 6047051717 {
            timest *= 1000;
        }

        if privmsg.channel != "#random" {
            return Ok(())
        }

        let mut arg_data = vec![];
        timest.encode_async(&mut arg_data).await.unwrap();
        ev.id().as_bytes().encode_async(&mut arg_data).await.unwrap();
        privmsg.nick.encode_async(&mut arg_data).await.unwrap();
        privmsg.msg.encode_async(&mut arg_data).await.unwrap();

        self.chatview_node.call_method("insert_line", arg_data).await.unwrap();

        Ok(())
    }

    async fn handle_send(&self) {
        // Get text from editbox
        let text = self.editbox_text.get();
        // Clear editbox
        self.editbox_text.set("");

        // Send text to channel
        debug!(target: "darkirc", "Sending privmsg: {text}");
        let msg = Privmsg::new("#random".to_string(), "anon".to_string(), text);
        let timestamp = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
        self.send_msg(timestamp, msg).await.unwrap();
    }
}
