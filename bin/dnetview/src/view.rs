use darkfi::error::Result;
use fxhash::{FxHashMap, FxHashSet};
use log::debug;
use serde::{Deserialize, Serialize};
use tui::widgets::ListState;

use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::model::{ConnectInfo, Model, NodeInfo, SelectableObject, SessionInfo};

#[derive(Debug, Clone)]
pub struct View {
    pub id_list: IdListView,
    pub info_list: NodeInfoView,
    pub selectables: FxHashMap<String, SelectableObject>,
}

impl View {
    pub fn new(
        id_list: IdListView,
        info_list: NodeInfoView,
        selectables: FxHashMap<String, SelectableObject>,
    ) -> View {
        View { id_list, info_list, selectables }
    }

    pub fn update_ids(&mut self, ids: FxHashSet<String>) {
        // TODO: check if it's active
        for id in ids {
            self.id_list.ids.insert(id);
        }
    }

    pub fn update_node_info(&mut self, nodes: FxHashMap<String, NodeInfo>) {
        for (id, node) in nodes {
            self.info_list.infos.insert(id, node);
        }
    }

    pub fn update_selectable(&mut self, selectables: FxHashMap<String, SelectableObject>) {
        for (id, obj) in selectables {
            self.selectables.insert(id, obj);
        }
    }

    // we only need to update NodeInfo
    //pub fn update(&mut self, model: Vec<SelectableObject>) -> Result<()> {
    //    for obj in model {
    //        let obj_clone = obj.clone();
    //        match obj {
    //            SelectableObject::Node(node) => {
    //                let node1 = node.clone();
    //                self.id_list.ids.insert(node1.clone().node_id);
    //                self.info_list.infos.insert(node.node_id, obj_clone);
    //            }
    //            SelectableObject::Session(session) => {
    //                let session1 = session.clone();
    //                // only write to the id list if not empty
    //                if !session.children.iter().all(|session| session.is_empty) {
    //                    self.id_list.ids.insert(session1.clone().session_id);
    //                }
    //                //self.id_list.ids.insert(session1.clone().session_id);
    //                self.info_list.infos.insert(session1.clone().session_id, obj_clone);
    //            }
    //            SelectableObject::Connect(connect) => {
    //                let connect1 = connect.clone();
    //                self.id_list.ids.insert(connect1.clone().connect_id);
    //                self.info_list.infos.insert(connect1.clone().connect_id, obj_clone);
    //            }
    //        }
    //    }
    //    //debug!("INFO LIST VIEW {:?}", self.info_list.infos);

    //    Ok(())
    //}

    pub fn render<B: Backend>(mut self, f: &mut Frame<'_, B>) {
        //debug!("VIEW AT RENDER {:?}", self.id_list.ids);
        //let mut nodes = Vec::new();
        let style = Style::default();
        let list_margin = 2;
        let list_direction = Direction::Horizontal;
        let list_cnstrnts = vec![Constraint::Percentage(50), Constraint::Percentage(50)];

        //let mut nodes = Vec::new();

        // this would return node info
        // for the left list
        for info in self.info_list.infos.values() {
            debug!("FOUND INFO: {:?}", info);
            // test
        }

        //for id in self.id_list.ids {
        //    match self.info_list.infos.get(&id) {
        //        Some(obj) => {
        //            match obj {
        //                SelectableObject::Node(info) => {
        //                    //let node_name = info.node_name.clone();
        //                    //draw_node(info.clone(), nodes.clone(), node_name);
        //                    let name_span = Span::raw(&info.node_name);
        //                    let lines = vec![Spans::from(name_span)];
        //                    let names = ListItem::new(lines);
        //                    nodes.push(names);
        //                    for child in &info.children {
        //                        //if !child.children.iter().all(|session| session.is_empty) {
        //                        //let name_span = Span::raw(&child.session_name);
        //                        let name =
        //                            Span::styled(format!("    {}", child.session_name), style);
        //                        let lines = vec![Spans::from(name)];
        //                        let names = ListItem::new(lines);
        //                        nodes.push(names);
        //                        for child in &child.children {
        //                            //let name_span = Span::raw(&child.connect_id);
        //                            let name =
        //                                Span::styled(format!("        {}", child.addr), style);
        //                            let lines = vec![Spans::from(name)];
        //                            let names = ListItem::new(lines);
        //                            nodes.push(names);
        //                        }
        //                        // thing
        //                        //}
        //                    }
        //                }
        //                SelectableObject::Session(info) => {
        //                    //let name_span = Span::raw(&info.session_name);
        //                    //let lines = vec![Spans::from(name_span)];
        //                    //let names = ListItem::new(lines);
        //                    //nodes.push(names);
        //                    //self.session_info.clone().render(info),
        //                }
        //                SelectableObject::Connect(info) => {
        //                    //let name_span = Span::raw(&info.connect_id);
        //                    //let lines = vec![Spans::from(name_span)];
        //                    //let names = ListItem::new(lines);
        //                    //nodes.push(names);
        //                    //self.connect_info.clone().render(info),
        //                }
        //            }
        //            //
        //        }
        //        None => {
        //            // TODO
        //        } //
        //    }
        //    //
        //}
        //let slice = Layout::default()
        //    .direction(list_direction)
        //    .margin(list_margin)
        //    .constraints(list_cnstrnts)
        //    .split(f.size());

        //let nodes =
        //    List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");

        //f.render_stateful_widget(nodes, slice[0], &mut self.id_list.state);
    }
}

fn draw_node(info: NodeInfo, mut nodes: Vec<ListItem>, node_name: String) {
    let style = Style::default();
    let name_span = Span::raw(node_name);
    let lines = vec![Spans::from(name_span)];
    let names = ListItem::new(lines);
    nodes.push(names);
    for child in &info.children {
        if !child.children.iter().all(|session| session.is_empty) {
            //let name_span = Span::raw(&child.session_name);
            let name = Span::styled(format!("    {}", child.session_name), style);
            let lines = vec![Spans::from(name)];
            let names = ListItem::new(lines);
            nodes.push(names);
            for child in &child.children {
                //let name_span = Span::raw(&child.connect_id);
                let name = Span::styled(format!("        {}", child.addr), style);
                let lines = vec![Spans::from(name)];
                let names = ListItem::new(lines);
                nodes.push(names);
            }
            // thing
        }
    }
    //nodes // thing
}

#[derive(Debug, Clone)]
pub struct IdListView {
    pub state: ListState,
    pub ids: FxHashSet<String>,
}

impl IdListView {
    pub fn new(ids: FxHashSet<String>) -> IdListView {
        IdListView { state: ListState::default(), ids }
    }
    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                debug!("INDEX: {}", i);
                if i >= self.ids.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        debug!("NEW INDEX: {}", i);
        debug!("IDS LEN: {}", self.ids.len());
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.ids.len() - 1
                } else {
                    debug!("NEW INDEX {}", i);
                    i - 1
                }
            }
            None => 0,
        };
        debug!("INDEX: {}", i);
        debug!("IDS LEN: {}", self.ids.len());
        self.state.select(Some(i));
    }

    pub fn unselect(&mut self) {
        self.state.select(None);
    }
}

#[derive(Debug, Clone)]
pub struct NodeInfoView {
    pub index: usize,
    pub infos: FxHashMap<String, NodeInfo>,
}

impl NodeInfoView {
    pub fn new(infos: FxHashMap<String, NodeInfo>) -> NodeInfoView {
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
