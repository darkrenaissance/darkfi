use crate::node_info::NodeInfo;

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

    pub fn next(&mut self) {
        self.index = (self.index + 1) % self.infos.len();
    }

    pub fn previous(&mut self) {
        if self.index > 0 {
            self.index -= 1;
        } else {
            self.index = self.infos.len() - 1;
        }
    }
}
