use crate::node::{NodeId, NodeInfo};
use std::collections::HashMap;
use tui::widgets::ListState;

#[derive(Clone)]
pub struct StatefulList {
    pub state: ListState,
    pub nodes: NodeId,
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
