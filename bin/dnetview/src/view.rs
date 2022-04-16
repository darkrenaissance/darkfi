use fxhash::{FxHashMap, FxHashSet};
//use log::debug;
use tui::widgets::ListState;

use crate::model::NodeInfo;

#[derive(Clone)]
pub struct View {
    pub id_list: IdListView,
    pub info_list: InfoListView,
}

impl View {
    pub fn new(id_list: IdListView, info_list: InfoListView) -> View {
        View { id_list, info_list }
    }

    pub fn update(&mut self, infos: FxHashMap<String, NodeInfo>) {
        for (id, info) in infos {
            self.id_list.node_id.insert(id.clone());
            self.info_list.infos.insert(id, info);
        }
    }
}

#[derive(Clone)]
pub struct IdListView {
    pub state: ListState,
    pub node_id: FxHashSet<String>,
}

impl IdListView {
    pub fn new(node_id: FxHashSet<String>) -> IdListView {
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
pub struct InfoListView {
    pub index: usize,
    pub infos: FxHashMap<String, NodeInfo>,
}

impl InfoListView {
    pub fn new(infos: FxHashMap<String, NodeInfo>) -> InfoListView {
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
}
