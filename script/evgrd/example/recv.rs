/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
    util::path::expand_path,
    Error, Result,
};
use darkfi_serial::{
    async_trait, deserialize_async_partial, AsyncDecodable, AsyncEncodable, SerialDecodable,
    SerialEncodable,
};
use sled_overlay::sled;
use smol::fs;
use tracing::{error, info};
use url::Url;

use evgrd::{FetchEventsMessage, LocalEventGraph, VersionMessage, MSG_EVENT, MSG_FETCHEVENTS};

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct Privmsg {
    pub channel: String,
    pub nick: String,
    pub msg: String,
}

async fn amain() -> Result<()> {
    info!("Instantiating event DAG");
    let ex = std::sync::Arc::new(smol::Executor::new());
    let datastore = expand_path("~/.local/share/darkfi/evgrd-test-client")?;
    fs::create_dir_all(&datastore).await?;
    let sled_db = sled::open(datastore)?;

    let evgr = LocalEventGraph::new(sled_db.clone(), "evgrd_testdag", 1, ex.clone()).await?;

    let endpoint = "tcp://127.0.0.1:5588";
    let endpoint = Url::parse(endpoint)?;

    let dialer = Dialer::new(endpoint, None).await?;
    let timeout = std::time::Duration::from_secs(60);

    println!("Connecting...");
    let mut stream = dialer.dial(Some(timeout)).await?;
    println!("Connected!");

    let version = VersionMessage::new();
    version.encode_async(&mut stream).await?;

    let server_version = VersionMessage::decode_async(&mut stream).await?;
    println!("Server version: {}", server_version.protocol_version);

    let unref_tips = evgr.unreferenced_tips.read().await.clone();
    let fetchevs = FetchEventsMessage::new(unref_tips);
    MSG_FETCHEVENTS.encode_async(&mut stream).await?;
    fetchevs.encode_async(&mut stream).await?;

    loop {
        let msg_type = u8::decode_async(&mut stream).await?;
        println!("Received: {msg_type:?}");
        if msg_type != MSG_EVENT {
            error!("Received invalid msg_type: {msg_type}");
            return Err(Error::MalformedPacket)
        }

        let ev = event_graph::Event::decode_async(&mut stream).await?;

        let genesis_timestamp = evgr.current_genesis.read().await.clone().timestamp;
        let ev_id = ev.id();
        if !evgr.dag.contains_key(ev_id.as_bytes()).unwrap() &&
            ev.validate(&evgr.dag, genesis_timestamp, evgr.days_rotation, None).await?
        {
            println!("got {ev:?}");
            evgr.dag_insert(&[ev.clone()]).await.unwrap();

            let privmsg: Privmsg = match deserialize_async_partial(ev.content()).await {
                Ok((v, _)) => v,
                Err(e) => {
                    println!("Failed deserializing incoming Privmsg event: {}", e);
                    continue
                }
            };

            println!("privmsg: {privmsg:?}");
        } else {
            println!("Event is invalid!")
        }
    }
}

fn main() {
    let _ = smol::block_on(amain());
}
