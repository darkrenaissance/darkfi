use async_std::sync::Mutex;
use darkfi::error::Result;
use log::debug;
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

    pub async fn update(self, infos: Vec<NodeInfo>) -> Result<()> {
        //for node in infos {
        //    self.info_list.infos.lock().await.push(node.clone());
        //    self.id_list.node_id.lock().await.push(node.clone().id);
        //}
        // Hashset will be a union
        Ok(())
    }
}

pub struct IdList {
    pub state: Mutex<ListState>,
    pub node_id: Mutex<HashSet<String>>,
}

impl IdList {
    // change vec to hashset
    pub fn new(node_id: HashSet<String>) -> IdList {
        let node_id = Mutex::new(node_id);
        IdList { state: Mutex::new(ListState::default()), node_id }
    }
}

pub struct InfoList {
    pub index: Mutex<usize>,
    // hashmap
    pub infos: Mutex<Vec<NodeInfo>>,
}

impl InfoList {
    pub fn new(infos: Vec<NodeInfo>) -> InfoList {
        let index = 0;
        let index = Mutex::new(index);
        let infos = Mutex::new(infos);

        InfoList { index, infos }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NodeInfo {
    pub id: String,
    pub outgoing: Vec<Connection>,
    pub incoming: Vec<Connection>,
}

impl NodeInfo {
    pub fn new() -> NodeInfo {
        NodeInfo { id: String::new(), outgoing: Vec::new(), incoming: Vec::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Connection {
    pub id: String,
    pub message: String,
}

impl Connection {
    pub fn new(id: String, message: String) -> Connection {
        Connection { id, message }
    }
}
