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

use darkfi::{
    event_graph::{self},
    net::transport::Dialer,
    system::ExecutorPtr,
    util::path::expand_path,
    Error, Result,
};
use darkfi_serial::{
    async_trait, deserialize_async_partial, AsyncDecodable, AsyncEncodable, Encodable,
    SerialDecodable, SerialEncodable,
};
use evgrd::{FetchEventsMessage, LocalEventGraph, VersionMessage, MSG_EVENT, MSG_FETCHEVENTS};
use log::{error, info};
use sled_overlay::sled;
use smol::fs;
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

pub async fn receive_msgs(sg_root: SceneNodePtr, ex: ExecutorPtr) -> Result<()> {
    let chatview_node = sg_root.lookup_node("/window/view/chatty").ok_or(Error::ConnectFailed)?;

    info!(target: "darkirc", "Instantiating DarkIRC event DAG");
    let datastore = expand_path(EVGRDB_PATH)?;
    fs::create_dir_all(&datastore).await?;
    let sled_db = sled::open(datastore)?;

    let evgr = LocalEventGraph::new(sled_db.clone(), "darkirc_dag", 1, ex.clone()).await?;

    let endpoint = "tcp://127.0.0.1:5588";
    let endpoint = Url::parse(endpoint)?;

    let dialer = Dialer::new(endpoint.clone(), None).await?;
    let timeout = std::time::Duration::from_secs(60);

    let mut stream = dialer.dial(Some(timeout)).await?;
    info!(target: "darkirc", "Connected to the backend: {endpoint}");

    let version = VersionMessage::new();
    version.encode_async(&mut stream).await?;

    let server_version = VersionMessage::decode_async(&mut stream).await?;
    info!(target: "darkirc", "Backend server version: {}", server_version.protocol_version);

    let unref_tips = evgr.unreferenced_tips.read().await.clone();
    let fetchevs = FetchEventsMessage::new(unref_tips);
    MSG_FETCHEVENTS.encode_async(&mut stream).await?;
    fetchevs.encode_async(&mut stream).await?;

    loop {
        let msg_type = u8::decode_async(&mut stream).await?;
        debug!(target: "darkirc", "Received: {msg_type:?}");
        if msg_type != MSG_EVENT {
            error!(target: "darkirc", "Received invalid msg_type: {msg_type}");
            return Err(Error::MalformedPacket)
        }

        let ev = event_graph::Event::decode_async(&mut stream).await?;

        let genesis_timestamp = evgr.current_genesis.read().await.clone().timestamp;
        let ev_id = ev.id();
        if evgr.dag.contains_key(ev_id.as_bytes()).unwrap() ||
            !ev.validate(&evgr.dag, genesis_timestamp, evgr.days_rotation, None).await?
        {
            error!(target: "darkirc", "Event is invalid! {ev:?}");
            continue
        }

        debug!(target: "darkirc", "got {ev:?}");
        evgr.dag_insert(&[ev.clone()]).await.unwrap();

        let privmsg: Privmsg = match deserialize_async_partial(ev.content()).await {
            Ok((v, _)) => v,
            Err(e) => {
                error!(target: "darkirc", "Failed deserializing incoming Privmsg event: {e}");
                continue
            }
        };

        debug!(target: "darkirc", "privmsg: {privmsg:?}");

        if privmsg.channel != "random" {
            continue
        }

        let response_fn = Box::new(|_| {});

        let mut arg_data = vec![];
        ev.timestamp.encode(&mut arg_data).unwrap();
        ev.id().as_bytes().encode(&mut arg_data).unwrap();
        privmsg.nick.encode(&mut arg_data).unwrap();
        privmsg.msg.encode(&mut arg_data).unwrap();

        chatview_node.call_method("insert_line", arg_data, response_fn).unwrap();
    }
}
