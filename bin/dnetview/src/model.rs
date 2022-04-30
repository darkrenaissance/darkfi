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
    pub node_info: Mutex<FxHashMap<String, NodeInfo>>,
    pub select_info: Mutex<FxHashMap<String, SelectableObject>>,
}

impl Model {
    pub fn new(
        ids: Mutex<FxHashSet<String>>,
        node_info: Mutex<FxHashMap<String, NodeInfo>>,
        select_info: Mutex<FxHashMap<String, SelectableObject>>,
    ) -> Model {
        Model { ids, node_info, select_info }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Hash)]
pub struct NodeInfo {
    pub node_id: String,
    pub node_name: String,
    pub children: Vec<SessionInfo>,
}

impl NodeInfo {
    pub fn new(node_id: String, node_name: String, children: Vec<SessionInfo>) -> NodeInfo {
        NodeInfo { node_id, node_name, children }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Hash)]
pub struct SessionInfo {
    pub session_name: String,
    pub session_id: String,
    pub parent: String,
    pub children: Vec<ConnectInfo>,
    pub is_empty: bool,
}

impl SessionInfo {
    pub fn new(
        session_name: String,
        session_id: String,
        parent: String,
        children: Vec<ConnectInfo>,
        is_empty: bool,
    ) -> SessionInfo {
        SessionInfo { session_name, session_id, parent, children, is_empty }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Hash)]
pub struct ConnectInfo {
    pub connect_id: String,
    pub addr: String,
    pub is_empty: bool,
    pub last_msg: String,
    pub last_status: String,
    pub state: String,
    pub msg_log: Vec<(String, String)>,
    pub parent: String,
}

impl ConnectInfo {
    pub fn new(
        connect_id: String,
        addr: String,
        is_empty: bool,
        last_msg: String,
        last_status: String,
        state: String,
        msg_log: Vec<(String, String)>,
        parent: String,
    ) -> ConnectInfo {
        ConnectInfo { connect_id, addr, is_empty, last_msg, last_status, state, msg_log, parent }
    }
}
