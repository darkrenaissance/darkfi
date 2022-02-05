use crate::{id_list::IdList, info_list::InfoList, node_info::NodeInfo};
//use async_std::sync::Mutex;
//use smol::Timer;
//use std::{collections::HashMap, time::Duration};

// make a structure to be able to modify and read them
// protect using a mutex
// arc reference
pub struct App {
    pub id_list: IdList,
    pub info_list: InfoList,
}

impl App {
    pub fn new(id_list: IdList, info_list: InfoList) -> App {
        //let infos = Vec::new();
        //let ids = Vec::new();

        //let info_list = InfoList::new(infos);
        //let id_list = IdList::new(ids);
        App { id_list, info_list }
    }

    // TODO: implement this
    //async fn sleep(self, dur: Duration) {
    //    Timer::after(dur).await;
    //}

    pub async fn update(mut self, node_vec: Vec<NodeInfo>) -> App {
        let ids = vec![node_vec[0].id.clone()];

        for id in ids {
            self.id_list.node_id.push(id);
        }

        let id_list = self.id_list;

        for info in node_vec {
            self.info_list.infos.push(info);
        }
        let info_list = self.info_list;

        App { id_list, info_list }
    }
}

//impl Default for App {
//    fn default() -> Self {
//        Self::new()
//    }
//}
