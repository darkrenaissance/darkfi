use async_std::sync::Mutex;

use fxhash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use darkfi::util::NanoTimestamp;

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
    Session(SessionInfo),
    Connect(ConnectInfo),
}

pub struct Model {
    pub ids: Mutex<FxHashSet<String>>,
    pub nodes: Mutex<FxHashMap<String, NodeInfo>>,
    pub msg_map: MsgMap,
    pub msg_log: Mutex<MsgLog>,
    pub selectables: Mutex<FxHashMap<String, SelectableObject>>,
}

impl Model {
    pub fn new(
        ids: Mutex<FxHashSet<String>>,
        nodes: Mutex<FxHashMap<String, NodeInfo>>,
        msg_map: MsgMap,
        msg_log: Mutex<MsgLog>,
        selectables: Mutex<FxHashMap<String, SelectableObject>>,
    ) -> Model {
        Model { ids, nodes, msg_map, msg_log, selectables }
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
    ) -> NodeInfo {
        NodeInfo { id, name, state, children, external_addr, is_offline }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Eq)]
pub struct SessionInfo {
    // TODO: make all values optional to handle empty sessions
    pub id: String,
    pub name: String,
    pub parent: String,
    pub is_empty: bool,
    pub children: Vec<ConnectInfo>,
    pub accept_addr: Option<String>,
}

impl SessionInfo {
    pub fn new(
        id: String,
        name: String,
        is_empty: bool,
        parent: String,
        children: Vec<ConnectInfo>,
        accept_addr: Option<String>,
    ) -> SessionInfo {
        SessionInfo { id, name, is_empty, parent, children, accept_addr }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Eq)]
pub struct ConnectInfo {
    // TODO: make all values optional to handle empty connections
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
    ) -> ConnectInfo {
        ConnectInfo {
            id,
            addr,
            state,
            parent,
            msg_log,
            is_empty,
            last_msg,
            last_status,
            remote_node_id,
        }
    }
}
