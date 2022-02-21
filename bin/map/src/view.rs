use crate::model::NodeInfo;
use log::debug;
use std::collections::HashSet;
use tui::widgets::ListState;

#[derive(Clone)]
pub struct View {
    pub id_list: IdListView,
    pub info_list: InfoListView,
}

impl View {
    pub fn new(id_list: IdListView, info_list: InfoListView) -> View {
        View { id_list, info_list }
    }

    // do we have this node id?
    //      if not, add everything
    //
    //      if yes, do we have these connections?
    //          if not, add everything
    //          if yes, do we have these msgs?
    //              if not, add
    pub fn update(&mut self, node_id: Vec<String>, infos: Vec<NodeInfo>) {
        // TODO: change nodeinfo.id to hashmap(k)
        //
        //let mut index = 0;
        for node in node_id.clone() {
            if !self.id_list.node_id.contains(&node) {
                self.id_list.update(node_id.clone());
                self.info_list.update(infos.clone());
            }
            //for info in self.info_list.infos.clone() {
            //    index = index + 1;
            //    if !info.outgoing.contains(&infos[index].outgoing[index]) {
            //        info.outgoing.update(&infos[index].outgoing[index])
            //    }
            //}
        }
        // all node ids that are not contained
        //debug!("UPDATE CALLED");
        //debug!("INFOS {:?}", infos);
        // node_id.union(self.id_list.node_id)
        //let new_node_ids =
        //    node_id.into_iter().filter(|id| !self.id_list.node_id.contains(id)).collect();
        ////[self.id_list.node_id, new_node_ids].concat();
        //self.id_list.update(new_node_ids);

        //// all node infos of node info that is different
        //// hashmap union
        //let mut new_node_info: HashSet<NodeInfo> = infos
        //    .into_iter()
        //    .filter_map(|id| {
        //        let opt_ni = self.info_list.infos.iter().find(|inf| inf.id == id.id);
        //        opt_ni.map(|ni| (ni, id))
        //    })
        //    .filter(|(view_ni, ni)| *view_ni != ni)
        //    .map(|(_, ni)| ni)
        //    .collect();

        //debug!("NEW NODE INFO: {:?}", new_node_info);
        //self.info_list.infos = self
        //    .info_list
        //    .infos
        //    .iter()
        //    .cloned()
        //    //.map(|i| new_node_info.find(|inf| i == *inf).map_or(i, |inf| inf))
        //    .map(|i| new_node_info.get(&i).map_or(i, |inf| inf.clone()))
        //    .collect();
        ////self.info_list.update(new_node_info);
    }
}

#[derive(Clone)]
pub struct IdListView {
    pub state: ListState,
    pub node_id: Vec<String>,
}

impl IdListView {
    pub fn new(node_id: Vec<String>) -> IdListView {
        IdListView { state: ListState::default(), node_id }
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
                debug!("if {} == 0", i);
                if i == 0 {
                    debug!("{} -1 ", self.node_id.len());
                    self.node_id.len() - 1
                } else {
                    debug!("else {} -1 ", i);
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

    pub fn update(&mut self, node_id: Vec<String>) {
        for id in node_id {
            self.node_id.push(id)
        }
    }
}

#[derive(Clone)]
pub struct InfoListView {
    pub index: usize,
    pub infos: Vec<NodeInfo>,
}

impl InfoListView {
    pub fn new(infos: Vec<NodeInfo>) -> InfoListView {
        let index = 0;

        InfoListView { index, infos }
    }

    pub async fn next(&mut self) {
        self.index = (self.index + 1) % self.infos.len();
    }

    // TODO: fix crash
    // == 0
    pub async fn previous(&mut self) {
        if self.index > 0 {
            self.index -= 1;
        } else {
            self.index = self.infos.len() - 1;
        }
    }

    pub fn update(&mut self, infos: Vec<NodeInfo>) {
        for info in infos {
            self.infos.push(info);
        }
    }
}
