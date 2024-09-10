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
    async_daemonize, cli_desc,
    event_graph::{self, proto::ProtocolEventGraph, EventGraph, EventGraphPtr},
    net::{
        session::SESSION_DEFAULT,
        settings::SettingsOpt as NetSettingsOpt,
        transport::{Dialer, Listener, PtListener, PtStream},
        P2p, P2pPtr,
    },
    rpc::{
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
    },
    system::{sleep, StoppableTask, StoppableTaskPtr},
    util::path::{expand_path, get_config_path},
    Error, Result,
};
use darkfi_serial::{
    async_trait, deserialize_async, serialize_async, AsyncDecodable, AsyncEncodable, Encodable,
    SerialDecodable, SerialEncodable,
};
use log::{debug, error, info, warn};
use url::Url;

use evgrd::{FetchEventsMessage, LocalEventGraph, VersionMessage, MSG_EVENT, MSG_FETCHEVENTS};

async fn amain() -> Result<()> {
    let evgr = LocalEventGraph::new();

    let endpoint = "tcp://127.0.0.1:5588";
    let endpoint = Url::parse(endpoint)?;

    let dialer = Dialer::new(endpoint, None).await?;
    let timeout = std::time::Duration::from_secs(60);

    info!("Connecting...");
    let mut stream = dialer.dial(Some(timeout)).await?;
    info!("Connected!");

    let version = VersionMessage::new();
    version.encode_async(&mut stream).await?;

    let server_version = VersionMessage::decode_async(&mut stream).await?;
    info!("Server version: {}", server_version.protocol_version);

    let fetchevs = FetchEventsMessage::new(evgr.unref_tips.clone());
    MSG_FETCHEVENTS.encode_async(&mut stream).await?;
    fetchevs.encode_async(&mut stream).await?;

    loop {
        let msg_type = u8::decode_async(&mut stream).await?;
        if msg_type != MSG_EVENT {
            error!("Received invalid msg_type: {msg_type}");
            return Err(Error::MalformedPacket)
        }

        let ev = event_graph::Event::decode_async(&mut stream).await?;
    }

    Ok(())
}

fn main() {
    smol::block_on(amain());
}
