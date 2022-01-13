use crate::types::{NodeId, NodeInfo};
use std::collections::HashMap;
use tui::widgets::ListState;

#[derive(Clone)]
pub struct NodeExtra {
    pub index: u32,
    pub noise: String,
}

impl NodeExtra {
    pub fn new() -> NodeExtra {
        let mut index = 0;
        NodeExtra { index, noise: String::new()}
    }
}
#[derive(Clone)]
pub struct StatefulList {
    pub state: ListState,
    pub nodes: HashMap<NodeId, NodeInfo>,
    pub nodex: NodeExtra,
}

impl StatefulList {
    pub fn new(nodes: HashMap<NodeId, NodeInfo>) -> StatefulList {
        StatefulList { state: ListState::default(), nodes, nodex: NodeExtra::new() }
    }

    pub fn next(&mut self) {
        let index = self.nodex.index;

        // index
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.nodes.len() - 1 {
                    //index = 0;
                    0
                } else {
                    //index + 1;
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
