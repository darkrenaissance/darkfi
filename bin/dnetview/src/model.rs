use async_std::sync::Mutex;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use tui::widgets::ListState;

pub struct Model {
    pub id_list: IdList,
    pub info_list: InfoList,
}

impl Model {
    pub fn new(id_list: IdList, info_list: InfoList) -> Model {
        Model { id_list, info_list }
    }
}

pub struct IdList {
    pub state: Mutex<ListState>,
    pub node_id: Mutex<HashSet<String>>,
}

impl IdList {
    pub fn new(node_id: HashSet<String>) -> IdList {
        let node_id = Mutex::new(node_id);
        IdList { state: Mutex::new(ListState::default()), node_id }
    }
}

pub struct InfoList {
    pub index: Mutex<usize>,
    pub infos: Mutex<HashMap<String, NodeInfo>>,
}

impl InfoList {
    pub fn new() -> InfoList {
        let index = 0;
        let index = Mutex::new(index);
        let infos = Mutex::new(HashMap::new());

        InfoList { index, infos }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NodeInfo {
    pub outbound: Vec<OutboundInfo>,
    pub manual: Vec<ManualInfo>,
    pub inbound: Vec<InboundInfo>,
}

impl NodeInfo {
    pub fn new() -> NodeInfo {
        NodeInfo { outbound: Vec::new(), manual: Vec::new(), inbound: Vec::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Eq, Hash)]
pub struct ManualInfo {
    pub key: u64,
}

impl ManualInfo {
    pub fn new(key: u64) -> ManualInfo {
        ManualInfo { key }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Eq, Hash)]
pub struct OutboundInfo {
    pub slots: Vec<Slot>,
}

impl OutboundInfo {
    pub fn new(slots: Vec<Slot>) -> OutboundInfo {
        OutboundInfo { slots }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Eq, Hash)]
pub struct Slot {
    pub addr: String,
    pub channel: Channel,
    pub state: String,
}

impl Slot {
    pub fn new(addr: String, channel: Channel, state: String) -> Slot {
        Slot { addr, channel, state }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash)]
pub struct Channel {
    pub last_msg: String,
    pub last_status: String,
}

impl Channel {
    pub fn new(last_msg: String, last_status: String) -> Channel {
        Channel { last_msg, last_status }
    }
}

#[derive(Clone, Deserialize, Debug, PartialEq, Eq, Hash)]
pub struct InboundInfo {
    pub connected: String,
    pub channel: Channel,
}

impl InboundInfo {
    pub fn new(connected: String, channel: Channel) -> InboundInfo {
        InboundInfo { connected, channel }
    }
}
