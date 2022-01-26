#[derive(Clone)]
pub struct NodeInfoView {
    pub index: usize,
    pub infos: Vec<NodeInfo>,
}

impl NodeInfoView {
    pub fn new(infos: Vec<NodeInfo>) -> NodeInfoView {
        let index = 0;

        NodeInfoView { index, infos }
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

//pub async fn add_seen(&self, id: u32) {
//    self.privmsg_ids.lock().await.insert(id);
//}
