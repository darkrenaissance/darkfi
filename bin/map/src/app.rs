use crate::{
    list::StatefulList,
    types::{NodeId, NodeInfo},
};
use std::collections::HashMap;

// the information here should be continually updating
// nodes are added from the result of rpc requests

#[derive(Clone)]
pub struct App {
    pub node_list: StatefulList,
}

impl App {
    pub fn new() -> App {
        let mut hashmap: HashMap<NodeId, NodeInfo> = HashMap::new();

        let id = Self::get_node_id();
        let info = Self::get_node_info();

        hashmap.insert(id, info);
        App { node_list: StatefulList::new(hashmap) }
    }

    fn get_node_id() -> String {
        let mut node_list = String::new();
        for num in 1..10000 {
            let new_nodes = format!("\n Node {}\n", num);
            node_list.push_str(&new_nodes);
        }
        node_list
    }

    fn get_node_info() -> String {
        let mut node_info = String::new();
        for num in 1..10000 {
            let new_info = format!(
                "\nConnections: {}\n
                \n Recent messages: ...",
                num
            );
            node_info.push_str(&new_info);
        }
        node_info
    }

    // TODO: implement this
    //fn update(&mut self) {
    //    let node = self.node.remove(0);
    //    self.nodes.push(node);
    //}
}
