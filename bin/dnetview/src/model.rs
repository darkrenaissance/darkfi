use async_std::sync::{Arc, Mutex};
use fxhash::FxHashMap;
use serde::{Deserialize, Serialize};

use darkfi::util::time::NanoTimestamp;

type MsgLog = Vec<(NanoTimestamp, String, String)>;
type MsgMap = Mutex<FxHashMap<String, MsgLog>>;

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
    Connect(ConnectInfo),
}

#[derive(Debug)]
pub struct Model {
    pub msg_map: MsgMap,
    pub msg_log: Mutex<MsgLog>,
    pub selectables: Mutex<FxHashMap<String, SelectableObject>>,
}

impl Model {
    pub fn new() -> Arc<Self> {
        let selectables = Mutex::new(FxHashMap::default());
        let msg_map = Mutex::new(FxHashMap::default());
        let msg_log = Mutex::new(Vec::new());
        Arc::new(Model { msg_map, msg_log, selectables })
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Eq)]
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub state: String,
    pub children: Vec<SessionInfo>,
    pub external_addr: Option<String>,
    pub is_offline: bool,
}

impl NodeInfo {
    pub fn new(
        id: String,
        name: String,
        state: String,
        children: Vec<SessionInfo>,
        external_addr: Option<String>,
        is_offline: bool,
    ) -> Self {
        Self { id, name, state, children, external_addr, is_offline }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Eq)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub parent: String,
    pub is_empty: bool,
    pub children: Vec<ConnectInfo>,
    pub accept_addr: Option<String>,
    pub hosts: Option<Vec<String>>,
}

impl SessionInfo {
    pub fn new(
        id: String,
        name: String,
        is_empty: bool,
        parent: String,
        children: Vec<ConnectInfo>,
        accept_addr: Option<String>,
        hosts: Option<Vec<String>>,
    ) -> Self {
        Self { id, name, is_empty, parent, children, accept_addr, hosts }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Eq)]
pub struct ConnectInfo {
    pub id: String,
    pub addr: String,
    pub state: String,
    pub parent: String,
    pub msg_log: Vec<(NanoTimestamp, String, String)>,
    pub is_empty: bool,
    pub last_msg: String,
    pub last_status: String,
    pub remote_node_id: String,
}

impl ConnectInfo {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        addr: String,
        state: String,
        parent: String,
        msg_log: Vec<(NanoTimestamp, String, String)>,
        is_empty: bool,
        last_msg: String,
        last_status: String,
        remote_node_id: String,
    ) -> Self {
        Self { id, addr, state, parent, msg_log, is_empty, last_msg, last_status, remote_node_id }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Eq)]
pub struct LilithInfo {
    pub id: String,
    pub name: String,
    pub urls: Vec<String>,
    pub networks: Vec<NetworkInfo>,
}

impl LilithInfo {
    pub fn new(id: String, name: String, urls: Vec<String>, networks: Vec<NetworkInfo>) -> Self {
        Self { id, name, urls, networks }
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
