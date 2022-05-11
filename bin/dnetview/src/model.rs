use async_std::sync::Mutex;

use fxhash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Hash)]
pub enum Session {
    Inbound,
    Outbound,
    Manual,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Hash)]
pub enum SelectableObject {
    Node(NodeInfo),
    Session(SessionInfo),
    Connect(ConnectInfo),
}

pub struct Model {
    pub ids: Mutex<FxHashSet<String>>,
    pub nodes: Mutex<FxHashMap<String, NodeInfo>>,
    pub msg_log: Mutex<FxHashMap<String, Vec<(u64, String, String)>>>,
    pub selectables: Mutex<FxHashMap<String, SelectableObject>>,
}

impl Model {
    pub fn new(
        ids: Mutex<FxHashSet<String>>,
        nodes: Mutex<FxHashMap<String, NodeInfo>>,
        msg_log: Mutex<FxHashMap<String, Vec<(u64, String, String)>>>,
        selectables: Mutex<FxHashMap<String, SelectableObject>>,
    ) -> Model {
        Model { ids, nodes, msg_log, selectables }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Hash)]
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub children: Vec<SessionInfo>,
    pub external_addr: String,
}

impl NodeInfo {
    pub fn new(
        id: String,
        name: String,
        children: Vec<SessionInfo>,
        external_addr: String,
    ) -> NodeInfo {
        NodeInfo { id, name, children, external_addr }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Hash)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub parent: String,
    pub is_empty: bool,
    pub children: Vec<ConnectInfo>,
}

impl SessionInfo {
    pub fn new(
        id: String,
        name: String,
        is_empty: bool,
        parent: String,
        children: Vec<ConnectInfo>,
    ) -> SessionInfo {
        SessionInfo { id, name, is_empty, parent, children }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Hash)]
pub struct ConnectInfo {
    pub id: String,
    pub addr: String,
    pub state: String,
    pub parent: String,
    pub msg_log: Vec<(u64, String, String)>,
    pub is_empty: bool,
    pub last_msg: String,
    pub last_status: String,
}

impl ConnectInfo {
    pub fn new(
        id: String,
        addr: String,
        state: String,
        parent: String,
        msg_log: Vec<(u64, String, String)>,
        is_empty: bool,
        last_msg: String,
        last_status: String,
    ) -> ConnectInfo {
        ConnectInfo { id, addr, state, parent, msg_log, is_empty, last_msg, last_status }
    }
}
