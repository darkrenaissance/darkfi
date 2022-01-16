use rand::Rng;

// TODO: make Node data structure
//       hashmap of node_id and node_info
//       make NodeInfoView that implements scrolling
//
// NodeInfoView
// next and previous updates index
// and updates info content
#[derive(Clone)]
pub struct NodeInfoView {
    pub index: usize,
}

impl NodeInfoView {
    pub fn new(infos: Vec<NodeInfo>) -> NodeInfoView {
        let index = 0;
        NodeInfoView { index }
    }

    //pub fn make_info() -> Vec<String> {
    //    let mut node_info = Vec::new();
    //    for num in 1..10 {
    //        let new_info = format!("Connections: {}", num);
    //        node_info.push(new_info.to_string());
    //    }
    //    node_info
    //}

    pub fn next(&mut self) {
        //self.index = (self.index + 1) % self.info.len();
    }

    pub fn previous(&mut self) {
        //if self.index > 0 {
        //    self.index -= 1;
        //} else {
        //    self.index = self.info.len() - 1;
        //}
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

    //pub fn make_info() -> Vec<String> {
    //    let mut node_info = Vec::new();
    //    for num in 1..10 {
    //        let new_info = format!("Connections: {}", num);
    //        node_info.push(new_info.to_string());
    //    }
    //    node_info
    //}

    //pub fn next(&mut self) {
    //    self.index = (self.index + 1) % self.info.len();
    //}

    //pub fn previous(&mut self) {
    //    if self.index > 0 {
    //        self.index -= 1;
    //    } else {
    //        self.index = self.info.len() - 1;
    //    }
    //}
}

// Make a string
// Type alias
//#[derive(Clone)]
//pub struct NodeId {
//    // TODO: make this plural
//    pub id: Vec<String>,
//}
//
//impl NodeId {
//    pub fn new() -> NodeId {
//        let id = Self::get_node_id();
//        NodeId { id }
//    }
//    fn get_node_id() -> Vec<String> {
//        let mut node_list = Vec::new();
//        for num in 1..100 {
//            let mut rng = rand::thread_rng();
//            let new_nodes = format!("Node: {}", rng.gen::<u32>());
//            node_list.push(new_nodes);
//        }
//        node_list
//    }
//}

// fn get_node_info() -> Vec<String> {
//     let mut node_info = Vec::new();
//     for _num in 1..100 {
//         //let new_info = format!("\nConnections: {}\n", num);
//         let new_info = "";
//         node_info.push(new_info.to_string());
//     }
//     node_info
// }
