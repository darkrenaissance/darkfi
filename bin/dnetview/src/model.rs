use async_std::sync::Mutex;

use fxhash::{FxHashMap, FxHashSet};
use serde::Deserialize;
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
    pub node_id: Mutex<FxHashSet<String>>,
}

impl IdList {
    pub fn new(node_id: FxHashSet<String>) -> IdList {
        let node_id = Mutex::new(node_id);
        IdList { state: Mutex::new(ListState::default()), node_id }
    }
}

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

impl Default for NodeInfo {
    fn default() -> Self {
        Self::new()
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
    pub is_empty: bool,
    pub slots: Vec<Slot>,
}

impl OutboundInfo {
    pub fn new(is_empty: bool, slots: Vec<Slot>) -> OutboundInfo {
        OutboundInfo { is_empty, slots }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Eq, Hash)]
pub struct Slot {
    pub is_empty: bool,
    pub addr: String,
    pub channel: Channel,
    pub state: String,
}

impl Slot {
    pub fn new(is_empty: bool, addr: String, channel: Channel, state: String) -> Slot {
        Slot { is_empty, addr, channel, state }
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
    pub is_empty: bool,
    pub connected: String,
    pub channel: Channel,
}

impl InboundInfo {
    pub fn new(is_empty: bool, connected: String, channel: Channel) -> InboundInfo {
        InboundInfo { is_empty, connected, channel }
    }
}
