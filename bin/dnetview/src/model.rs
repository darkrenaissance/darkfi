use async_std::sync::Mutex;

use fxhash::{FxHashMap, FxHashSet};
use serde::Deserialize;
use tui::widgets::ListState;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SelectableObject {
    Node(NodeInfo),
    Session(SessionInfo),
    Connect(ConnectInfo),
}

pub struct Model {
    pub id_set: Mutex<FxHashSet<u32>>,
    pub node_info: Mutex<FxHashMap<u32, SelectableObject>>,
    pub session_info: Mutex<FxHashMap<u32, SelectableObject>>,
    pub connect_info: Mutex<FxHashMap<u32, SelectableObject>>,
}

impl Model {
    pub fn new(
        id_set: Mutex<FxHashSet<u32>>,
        node_info: Mutex<FxHashMap<u32, SelectableObject>>,
        session_info: Mutex<FxHashMap<u32, SelectableObject>>,
        connect_info: Mutex<FxHashMap<u32, SelectableObject>>,
    ) -> Model {
        Model { id_set, node_info, session_info, connect_info }
    }
}

//pub struct IdList {
//    pub state: Mutex<ListState>,
//    pub node_id: Mutex<FxHashSet<String>>,
//}
//
//impl IdList {
//    pub fn new(node_id: FxHashSet<String>) -> IdList {
//        let node_id = Mutex::new(node_id);
//        IdList { state: Mutex::new(ListState::default()), node_id }
//    }
//}

pub struct InfoList {
    pub index: Mutex<usize>,
    pub infos: Mutex<FxHashMap<String, NodeInfo>>,
}

impl InfoList {
    pub fn new() -> InfoList {
        let index = 0;
        let index = Mutex::new(index);
        let infos = Mutex::new(FxHashMap::default());

        InfoList { index, infos }
    }
}

impl Default for InfoList {
    fn default() -> Self {
        Self::new()
    }
}

type NodeId = u32;
type SessionId = u32;
type ConnectId = u32;

#[derive(Clone, Deserialize, Debug, PartialEq, Eq, Hash)]
pub struct NodeInfo {
    pub node_id: NodeId,
    pub node_name: String,
    pub children: Vec<SessionInfo>,
}

impl NodeInfo {
    pub fn new(node_id: NodeId, node_name: String, children: Vec<SessionInfo>) -> NodeInfo {
        NodeInfo { node_id, node_name, children }
    }
}

#[derive(Clone, Deserialize, Debug, PartialEq, Eq, Hash)]
pub struct SessionInfo {
    pub session_id: SessionId,
    pub parent: NodeId,
    pub children: Vec<ConnectInfo>,
}

impl SessionInfo {
    pub fn new(session_id: SessionId, parent: NodeId, children: Vec<ConnectInfo>) -> SessionInfo {
        SessionInfo { session_id, parent, children }
    }
}

#[derive(Clone, Deserialize, Debug, PartialEq, Eq, Hash)]
pub struct ConnectInfo {
    pub connect_id: ConnectId,
    pub addr: String,
    pub is_empty: bool,
    pub last_msg: String,
    pub last_status: String,
    pub state: String,
    pub msg_log: Vec<String>,
    pub parent: SessionId,
}

impl ConnectInfo {
    pub fn new(
        connect_id: ConnectId,
        addr: String,
        is_empty: bool,
        last_msg: String,
        last_status: String,
        state: String,
        msg_log: Vec<String>,
        parent: SessionId,
    ) -> ConnectInfo {
        ConnectInfo { connect_id, addr, is_empty, last_msg, last_status, state, msg_log, parent }
    }
}
