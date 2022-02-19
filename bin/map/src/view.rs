use crate::model::NodeInfo;
use log::debug;
use tui::widgets::ListState;

#[derive(Clone)]
pub struct View {
    pub id_list: IdListView,
    pub info_list: InfoListView,
}

impl View {
    pub fn new(id_list: IdListView, info_list: InfoListView) -> View {
        View { id_list, info_list }
    }

    pub fn update(&mut self, node_id: Vec<String>, infos: Vec<NodeInfo>) {
        for node in node_id.clone() {
            if !self.id_list.node_id.contains(&node) {
                self.id_list.update(node_id.clone());
                self.info_list.update(infos.clone());
            }
        }
    }
}

#[derive(Clone)]
pub struct IdListView {
    pub state: ListState,
    pub node_id: Vec<String>,
}

impl IdListView {
    pub fn new(node_id: Vec<String>) -> IdListView {
        IdListView { state: ListState::default(), node_id }
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
                debug!("if {} == 0", i);
                if i == 0 {
                    debug!("{} -1 ", self.node_id.len());
                    self.node_id.len() - 1
                } else {
                    debug!("else {} -1 ", i);
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

    pub fn update(&mut self, node_id: Vec<String>) {
        for id in node_id {
            self.node_id.push(id)
        }
    }
}

#[derive(Clone)]
pub struct InfoListView {
    pub index: usize,
    pub infos: Vec<NodeInfo>,
}

impl InfoListView {
    pub fn new(infos: Vec<NodeInfo>) -> InfoListView {
        let index = 0;

        InfoListView { index, infos }
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

    pub fn update(&mut self, infos: Vec<NodeInfo>) {
        for info in infos {
            self.infos.push(info);
        }
    }
}
