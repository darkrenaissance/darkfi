use async_std::sync::Mutex;

use fxhash::{FxHashMap, FxHashSet};
use serde::Deserialize;

type NodeId = u64;
type SessionId = u64;
type ConnectId = u64;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SelectableObject {
    Node(NodeInfo),
    Session(SessionInfo),
    Connect(ConnectInfo),
}

pub struct Model {
    pub ids: Mutex<FxHashSet<u64>>,
    pub infos: Mutex<FxHashMap<u64, SelectableObject>>,
}

impl Model {
    pub fn new(
        ids: Mutex<FxHashSet<u64>>,
        infos: Mutex<FxHashMap<u64, SelectableObject>>,
    ) -> Model {
        Model { ids, infos }
    }
}

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
