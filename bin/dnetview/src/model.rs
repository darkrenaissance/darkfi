/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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
use std::collections::HashMap;

use async_std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};

use darkfi::util::time::NanoTimestamp;

type MsgLog = Vec<(NanoTimestamp, String, String)>;
type MsgMap = Mutex<HashMap<String, MsgLog>>;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Eq)]
pub enum Session {
    Inbound,
    Outbound,
    Manual,
    Offline,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Eq)]
pub enum SelectableObject {
    Node(NodeInfo),
    Lilith(LilithInfo),
    Network(NetworkInfo),
    Session(SessionInfo),
    Connect(SlotInfo),
}

#[derive(Debug)]
pub struct Model {
    pub msg_map: MsgMap,
    pub log: Mutex<MsgLog>,
    pub selectables: Mutex<HashMap<String, SelectableObject>>,
}

impl Model {
    pub fn new() -> Arc<Self> {
        let selectables = Mutex::new(HashMap::new());
        let msg_map = Mutex::new(HashMap::new());
        let log = Mutex::new(Vec::new());
        Arc::new(Model { msg_map, log, selectables })
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Eq)]
pub struct NodeInfo {
    pub dnet_id: String,
    pub name: String,
    pub hosts: Vec<String>,
    pub info: Vec<SessionInfo>,
    pub is_offline: bool,
}

impl NodeInfo {
    pub fn new(
        dnet_id: String,
        name: String,
        hosts: Vec<String>,
        info: Vec<SessionInfo>,
        is_offline: bool,
    ) -> Self {
        Self { dnet_id, name, hosts, info, is_offline }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Eq)]
pub struct SessionInfo {
    pub dnet_id: String,
    pub node_id: String,
    pub name: String,
    pub addr: String,
    pub state: String,
    pub info: Vec<SlotInfo>,
    pub is_empty: bool,
}

impl SessionInfo {
    pub fn new(
        dnet_id: String,
        node_id: String,
        name: String,
        addr: String,
        state: String,
        info: Vec<SlotInfo>,
        is_empty: bool,
    ) -> Self {
        Self { dnet_id, node_id, name, addr, state, info, is_empty }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Eq)]
pub struct SlotInfo {
    pub dnet_id: String,
    pub node_id: String,
    pub addr: String,
    pub random_id: String,
    pub remote_id: String,
    pub log: Vec<(NanoTimestamp, String, String)>,
    pub is_empty: bool,
}

impl SlotInfo {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dnet_id: String,
        node_id: String,
        addr: String,
        random_id: String,
        remote_id: String,
        log: Vec<(NanoTimestamp, String, String)>,
        is_empty: bool,
    ) -> Self {
        Self {
            dnet_id,
            addr,
            random_id,
            remote_id,
            log,
            node_id,
            is_empty,
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Eq)]
pub struct LilithInfo {
    pub id: String,
    pub name: String,
    pub networks: Vec<NetworkInfo>,
}

impl LilithInfo {
    pub fn new(id: String, name: String, networks: Vec<NetworkInfo>) -> Self {
        Self { id, name, networks }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Eq)]
pub struct NetworkInfo {
    pub id: String,
    pub name: String,
    pub urls: Vec<String>,
    pub nodes: Vec<String>,
}

impl NetworkInfo {
    pub fn new(id: String, name: String, urls: Vec<String>, nodes: Vec<String>) -> Self {
        Self { id, name, urls, nodes }
    }
}
