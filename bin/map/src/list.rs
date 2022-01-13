use crate::types::{NodeId, NodeInfo};
use std::collections::HashMap;
use tui::widgets::ListState;

//pub struct NodeInfo {
//    pub index: u32
//}

#[derive(Clone)]
pub struct StatefulList {
    pub state: ListState,
    pub nodes: HashMap<NodeId, NodeInfo>,
}

impl StatefulList {
    pub fn new(nodes: HashMap<NodeId, NodeInfo>) -> StatefulList {
        StatefulList { state: ListState::default(), nodes }
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.nodes.len() - 1 {
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
                    self.nodes.len() - 1
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
