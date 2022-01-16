use rand::Rng;

#[derive(Clone)]
pub struct NodeInfo {
    pub info: Vec<String>,
    pub index: usize,
}

impl NodeInfo {
    pub fn new() -> NodeInfo {
        let info = Self::make_info();
        let index = 0;
        NodeInfo { info, index }
    }

    // set content
    fn make_info() -> Vec<String> {
        let mut node_info = Vec::new();
        for num in 1..3 {
            let new_info = format!(
                "Connections: {}
                                   ",
                num
            );
            node_info.push(new_info.to_string());
        }
        node_info
    }

    pub fn next(&mut self) {
        self.index = (self.index + 1) % self.info.len();
    }

    pub fn previous(&mut self) {
        if self.index > 0 {
            self.index -= 1;
        } else {
            self.index = self.info.len() - 1;
        }
    }
}

#[derive(Clone)]
pub struct NodeId {
    pub id: Vec<String>,
}

impl NodeId {
    pub fn new() -> NodeId {
        let id = Self::get_node_id();
        NodeId { id }
    }
    fn get_node_id() -> Vec<String> {
        let mut node_list = Vec::new();
        for num in 1..10 {
            let mut rng = rand::thread_rng();
            let new_nodes = format!("Node: {}", rng.gen::<u32>());
            node_list.push(new_nodes);
        }
        node_list
    }
}

// fn get_node_info() -> Vec<String> {
//     let mut node_info = Vec::new();
//     for _num in 1..100 {
//         //let new_info = format!("\nConnections: {}\n", num);
//         let new_info = "";
//         node_info.push(new_info.to_string());
//     }
//     node_info
// }
