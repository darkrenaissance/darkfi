use tui::widgets::ListState;

#[derive(Clone)]
pub struct Model {
    pub id_list: IdList,
    pub info_list: InfoList,
}

impl Model {
    pub fn new(id_list: IdList, info_list: InfoList) -> Model {
        Model { id_list, info_list }
    }

    pub async fn update(mut self, node_vec: Vec<NodeInfo>) -> Model {
        let ids = vec![node_vec[0].id.clone()];

        for id in ids {
            self.id_list.node_id.push(id);
        }

        let id_list = self.id_list;

        for info in node_vec {
            self.info_list.infos.push(info);
        }
        let info_list = self.info_list;

        Model { id_list, info_list }
    }
}

#[derive(Clone)]
pub struct IdList {
    pub state: ListState,
    pub node_id: Vec<String>,
}

impl IdList {
    pub fn new(node_id: Vec<String>) -> IdList {
        IdList { state: ListState::default(), node_id }
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.node_id.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.node_id.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn unselect(&mut self) {
        self.state.select(None);
    }
}

#[derive(Clone)]
pub struct InfoList {
    pub index: usize,
    pub infos: Vec<NodeInfo>,
}

impl InfoList {
    pub fn new(infos: Vec<NodeInfo>) -> InfoList {
        let index = 0;

        InfoList { index, infos }
    }

    pub async fn next(&mut self) {
        self.index = (self.index + 1) % self.infos.len();
    }

    pub async fn previous(&mut self) {
        if self.index > 0 {
            self.index -= 1;
        } else {
            self.index = self.infos.len() - 1;
        }
    }
}

pub type NodeId = u32;

#[derive(Clone)]
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
