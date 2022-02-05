//TODO: made node_id into a hashset across project
//use std::collections::HashSet
use tui::widgets::ListState;

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

//pub async fn add_seen(&self, id: u32) {
//    self.privmsg_ids.lock().await.insert(id);
//}
