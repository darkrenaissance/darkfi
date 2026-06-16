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

use sled_overlay::sled;
use smol::lock::Mutex;
use std::{collections::HashSet, path::PathBuf};

use darkfi::{
    event_graph::EventGraphPtr, net::P2pPtr, rpc::jsonrpc::JsonSubscriber, system::StoppableTaskPtr,
};
use darkfi_serial::{async_trait, SerialDecodable, SerialEncodable};

/// IRC server and client handler implementation
pub mod irc;
use irc::server::IrcServer;

use crate::irc::server::MAX_NICK_LEN;

/// Cryptography utilities
pub mod crypto;

/// JSON-RPC methods
pub mod rpc;

/// Settings utilities
pub mod settings;

/// IRC PRIVMSG
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct Privmsg {
    pub version: u8,
    pub msg_type: u8,
    pub channel: String,
    pub nick: String,
    pub msg: String,
}

pub struct DarkIrc {
    /// P2P network pointer
    p2p: P2pPtr,
    /// Sled DB (also used in event_graph and for RLN)
    sled: sled::Db,
    /// Event Graph instance
    event_graph: EventGraphPtr,
    /// JSON-RPC connection tracker
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// dnet JSON-RPC subscriber
    dnet_sub: JsonSubscriber,
    /// deg JSON-RPC subscriber
    deg_sub: JsonSubscriber,
    /// Replay logs (DB) path
    replay_datastore: PathBuf,
}

impl DarkIrc {
    pub fn new(
        p2p: P2pPtr,
        sled: sled::Db,
        event_graph: EventGraphPtr,
        dnet_sub: JsonSubscriber,
        deg_sub: JsonSubscriber,
        replay_datastore: PathBuf,
    ) -> Self {
        Self {
            p2p,
            sled,
            event_graph,
            rpc_connections: Mutex::new(HashSet::new()),
            dnet_sub,
            deg_sub,
            replay_datastore,
        }
    }
}

pub fn pad(string: &str) -> Vec<u8> {
    let mut bytes = string.as_bytes().to_vec();
    bytes.resize(MAX_NICK_LEN, 0x00);
    bytes
}

pub fn unpad(vec: &mut Vec<u8>) {
    if let Some(i) = vec.iter().rposition(|x| *x != 0) {
        let new_len = i + 1;
        vec.truncate(new_len);
    }
}
