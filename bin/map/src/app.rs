use crate::{
    list::StatefulList,
    node::{NodeId, NodeInfo},
};
use rand::Rng;
use smol::Timer;
use std::{collections::HashMap, time::Duration};

#[derive(Clone)]
pub struct App {
    pub node_list: StatefulList,
    pub node_info: NodeInfo,
}

impl App {
    pub fn new() -> App {
        let node_info = NodeInfo::new();
        let node_id = NodeId::new();
        let node_list = StatefulList::new(node_id);
        App { node_list, node_info }
    }

    // TODO: implement this
    //async fn sleep(self, dur: Duration) {
    //    Timer::after(dur).await;
    //}

    //pub async fn update(mut self) {
    //    self.node_list.nodes.insert("New node joined".to_string(), "".to_string());
    //    //self.sleep(Duration::from_secs(2)).await;
    //}
}
