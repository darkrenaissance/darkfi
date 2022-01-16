use crate::{
    node::{NodeId, NodeInfo},
    ui::render_selected,
};
use std::collections::HashMap;
use tui::widgets::ListState;

// TODO: make this just a list
// hashmaps are owned by App
#[derive(Clone)]
pub struct StatefulList {
    pub state: ListState,
    pub nodes: NodeId,
    //pub nodes: HashMap<NodeId, NodeInfo>,
    //pub node_info: NodeInfo,
    //pub index: HashMap<usize, NodeInfo>,
    //pub node_info: InfoScreen,
}

impl StatefulList {
    pub fn new(nodes: NodeId) -> StatefulList {
        StatefulList { state: ListState::default(), nodes }
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.nodes.id.len() - 1 {
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
                    self.nodes.id.len() - 1
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
