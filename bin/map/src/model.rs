use async_std::sync::Mutex;
use darkfi::error::{Error, Result};
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
        for node in infos {
            self.info_list.infos.lock().await.push(node.clone());
            self.id_list.node_id.lock().await.push(node.clone().id);
        }
        Ok(())
    }
}

pub struct IdList {
    pub state: Mutex<ListState>,
    pub node_id: Mutex<Vec<String>>,
}

impl IdList {
    pub fn new(node_id: Vec<String>) -> IdList {
        let node_id = Mutex::new(node_id);
        IdList { state: Mutex::new(ListState::default()), node_id }
    }
}

pub struct InfoList {
    pub index: Mutex<usize>,
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

//pub type NodeId = u32;

#[derive(Clone, Debug)]
pub struct NodeInfo {
    pub id: String,
    pub connections: usize,
    pub is_active: bool,
    pub last_message: String,
}

impl NodeInfo {
    pub fn new() -> NodeInfo {
        let connections = 0;
        let is_active = false;
        NodeInfo { id: String::new(), connections, is_active, last_message: String::new() }
    }
}

impl Default for NodeInfo {
    fn default() -> Self {
        Self::new()
    }
}
