use crate::{
    list::NodeIdList,
    node_info::{NodeInfo, NodeInfoView},
};
//use smol::Timer;
//use std::{collections::HashMap, time::Duration};

// make a structure to be able to modify and read them
// protect using a mutex
// arc reference
#[derive(Clone)]
pub struct App {
    pub node_list: NodeIdList,
    pub node_info: NodeInfoView,
}

impl App {
    pub fn new() -> App {
        // append to vector of node info
        let infos = vec![
            NodeInfo {
                id: "0385048034sodisofjhosd1111q3434".to_string(),
                connections: 10,
                is_active: true,
                last_message: "hey how are you?".to_string(),
            },
            NodeInfo {
                id: "09w30we9wsnfksdfkdjflsjkdfjdfsd".to_string(),
                connections: 5,
                is_active: false,
                last_message: "lmao".to_string(),
            },
            NodeInfo {
                id: "038043325alsdlasjfrsdfsdfsdjsdf".to_string(),
                connections: 7,
                is_active: true,
                last_message: "gm".to_string(),
            },
            NodeInfo {
                id: "04985034953ldflsdjflsdjflsdjfii".to_string(),
                connections: 2,
                is_active: true,
                last_message: "hihi".to_string(),
            },
            NodeInfo {
                id: "09850249352asdjapsdikalskasdkas".to_string(),
                connections: 10,
                is_active: true,
                last_message: "wtf".to_string(),
            },
        ];

        let node_info = NodeInfoView::new(infos.clone());

        let ids = vec![
            infos[0].id.clone(),
            infos[1].id.clone(),
            infos[2].id.clone(),
            infos[3].id.clone(),
            infos[4].id.clone(),
        ];

        let node_list = NodeIdList::new(ids);
        App { node_list, node_info }
    }

    // TODO: implement this
    //async fn sleep(self, dur: Duration) {
    //    Timer::after(dur).await;
    //}

    pub async fn update(mut self, node_vec: Vec<NodeInfo>) -> App {
        let node_info = NodeInfoView::new(node_vec.clone());

        let ids = vec![node_vec[0].id.clone(), node_vec[1].id.clone(), node_vec[2].id.clone()];

        let node_list = NodeIdList::new(ids);
        App { node_list, node_info }
    }
}
