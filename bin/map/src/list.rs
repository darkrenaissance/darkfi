use crate::{
    info::NodeExtra,
    types::{NodeId, NodeInfo},
};
use std::collections::HashMap;
use tui::widgets::ListState;

#[derive(Clone)]
pub struct StatefulList {
    pub state: ListState,
    pub nodes: HashMap<NodeId, NodeInfo>,
    pub node_info: NodeExtra,
}

impl StatefulList {
    pub fn new(nodes: HashMap<NodeId, NodeInfo>, node_info: NodeExtra) -> StatefulList {
        StatefulList { state: ListState::default(), nodes, node_info }
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
