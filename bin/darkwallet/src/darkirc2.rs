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
    async_trait, deserialize_async_partial, AsyncDecodable, AsyncEncodable, Encodable,
    SerialDecodable, SerialEncodable,
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
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex as SyncMutex, Weak,
};
use url::Url;

use crate::scene::SceneNodePtr;

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
    receive_task: SyncMutex<Option<smol::Task<()>>>,

    chatview_node: SceneNodePtr,
}

impl LocalDarkIRC {
    pub async fn new(sg_root: SceneNodePtr, ex: ExecutorPtr) -> Result<Arc<Self>> {
        let chatview_node = sg_root.lookup_node("/window/view/chatty").unwrap();

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
            receive_task: SyncMutex::new(None),

            chatview_node,
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

        self.version_exchange().await?;

        let me = Arc::downgrade(&self);
        let task = ex.spawn(async move {
            while let Some(self_) = me.upgrade() {
                self_.receive_msg().await.unwrap();
            }
            error!(target: "darkirc", "Closing DarkIRC receive loop");
        });

        let mut receive_task = self.receive_task.lock().unwrap();
        assert!(receive_task.is_none());
        *receive_task = Some(task);

        self.is_connected.store(true, Ordering::Relaxed);

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

        Ok(())
    }

    async fn send_msg(&self, timestamp: u64, msg: Privmsg) -> Result<()> {
        if !self.is_connected.load(Ordering::Relaxed) {
            self.reconnect().await?;
        }

        let mut writer = self.writer.lock().await;
        let writer = writer.as_mut().unwrap();

        MSG_SENDEVENT.encode_async(writer).await?;
        timestamp.encode_async(writer).await?;
        msg.encode_async(writer).await?;
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

        if privmsg.channel != "#random" {
            return Ok(())
        }

        let mut arg_data = vec![];
        ev.timestamp.encode_async(&mut arg_data).await.unwrap();
        ev.id().as_bytes().encode_async(&mut arg_data).await.unwrap();
        privmsg.nick.encode_async(&mut arg_data).await.unwrap();
        privmsg.msg.encode_async(&mut arg_data).await.unwrap();

        self.chatview_node.call_method("insert_line", arg_data).await.unwrap();

        Ok(())
    }
}
