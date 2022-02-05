//TODO: made node_id into a HashSet(u32)
// wrap NodeInfo and NodeId in a Mutex

pub type NodeId = u32;

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
