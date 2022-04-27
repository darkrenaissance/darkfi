use async_std::sync::Arc;
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
    pub info_list: InfoListView,
    pub node_info: NodeInfoView,
    pub session_info: SessionInfoView,
    pub connect_info: ConnectInfoView,
}

impl View {
    pub fn new(
        id_list: IdListView,
        info_list: InfoListView,
        node_info: NodeInfoView,
        session_info: SessionInfoView,
        connect_info: ConnectInfoView,
    ) -> View {
        View { id_list, info_list, node_info, session_info, connect_info }
    }

    pub fn update(&mut self, model: Vec<SelectableObject>) -> Result<()> {
        for obj in model {
            let obj_clone = obj.clone();
            match obj {
                SelectableObject::Node(node) => {
                    let node1 = node.clone();
                    self.node_info.clone().update(node1.clone())?;
                    self.id_list.ids.insert(node1.clone().node_id);
                    self.info_list.infos.insert(node.node_id, obj_clone);
                }
                SelectableObject::Session(session) => {
                    let session1 = session.clone();
                    self.session_info.clone().update(session1.clone())?;
                    self.id_list.ids.insert(session1.clone().session_id);
                    self.info_list.infos.insert(session1.clone().session_id, obj_clone);
                }
                SelectableObject::Connect(connect) => {
                    let connect1 = connect.clone();
                    self.connect_info.clone().update(connect)?;
                    self.id_list.ids.insert(connect1.clone().connect_id);
                    self.info_list.infos.insert(connect1.clone().connect_id, obj_clone);
                }
            }
        }

        Ok(())
    }

    pub fn render<B: Backend>(mut self, f: &mut Frame<'_, B>) {
        //debug!("VIEW AT RENDER {:?}", self.id_list.ids);
        //let mut nodes = Vec::new();
        let style = Style::default();
        let list_margin = 2;
        let list_direction = Direction::Horizontal;
        let list_cnstrnts = vec![Constraint::Percentage(50), Constraint::Percentage(50)];

        let mut nodes = Vec::new();

        for id in self.id_list.ids {
            match self.info_list.infos.get(&id) {
                Some(obj) => {
                    match obj {
                        SelectableObject::Node(info) => {
                            let name_span = Span::raw(&info.node_name);
                            let lines = vec![Spans::from(name_span)];
                            let names = ListItem::new(lines);
                            nodes.push(names);
                            for child in &info.children {
                                //let name_span = Span::raw(&child.session_name);
                                let name =
                                    Span::styled(format!("    {}", child.session_name), style);
                                let lines = vec![Spans::from(name)];
                                let names = ListItem::new(lines);
                                nodes.push(names);
                                for child in &child.children {
                                    //let name_span = Span::raw(&child.connect_id);
                                    let name = Span::styled(
                                        format!("        {}", child.connect_id),
                                        style,
                                    );
                                    let lines = vec![Spans::from(name)];
                                    let names = ListItem::new(lines);
                                    nodes.push(names);
                                }
                            }
                        }
                        SelectableObject::Session(info) => {
                            //let name_span = Span::raw(&info.session_name);
                            //let lines = vec![Spans::from(name_span)];
                            //let names = ListItem::new(lines);
                            //nodes.push(names);
                            //self.session_info.clone().render(info),
                        }
                        SelectableObject::Connect(info) => {
                            //let name_span = Span::raw(&info.connect_id);
                            //let lines = vec![Spans::from(name_span)];
                            //let names = ListItem::new(lines);
                            //nodes.push(names);
                            //self.connect_info.clone().render(info),
                        }
                    }
                    //
                }
                None => {
                    // TODO
                } //
            }
            //
        }
        let slice = Layout::default()
            .direction(list_direction)
            .margin(list_margin)
            .constraints(list_cnstrnts)
            .split(f.size());

        let nodes =
            List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");

        f.render_stateful_widget(nodes, slice[0], &mut self.id_list.state);
        //for node in nodes {
        //    f.render_stateful_widget(node, slice[0], &mut self.id_list.state);
        //}
    }
}

#[derive(Debug, Clone)]
pub struct NodeInfoView {
    pub node_id: String,
    pub node_name: String,
    pub children: Vec<SessionInfo>,
}

impl NodeInfoView {
    pub fn default() -> NodeInfoView {
        let node_id = String::new();
        let node_name = String::new();
        let children: Vec<SessionInfo> = Vec::new();
        NodeInfoView { node_id, node_name, children }
    }

    pub fn update(mut self, data: NodeInfo) -> Result<()> {
        //self.state.slect(Some(0));
        self.node_id = data.node_id;
        self.node_name = data.node_name;
        self.children = data.children;
        Ok(())
    }

    pub fn render(self, node: &NodeInfo) -> List {
        let mut nodes = Vec::new();
        let mut lines: Vec<Spans> = Vec::new();
        let name_span = Span::raw(&node.node_name);
        lines.push(Spans::from(name_span));
        let ids = ListItem::new(lines);
        nodes.push(ids);
        let nodes =
            List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");
        nodes
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Hash)]
pub struct SessionInfoView {
    pub session_name: String,
    pub session_id: String,
    pub parent: String,
    pub children: Vec<ConnectInfo>,
}

impl SessionInfoView {
    pub fn default() -> SessionInfoView {
        let session_name = String::new();
        let session_id = String::new();
        let parent = String::new();
        let children: Vec<ConnectInfo> = Vec::new();
        SessionInfoView { session_name, session_id, parent, children }
    }

    pub fn update(mut self, data: SessionInfo) -> Result<()> {
        self.session_name = data.session_name;
        self.session_id = data.session_id;
        self.parent = data.parent;
        self.children = data.children;
        Ok(())
    }

    pub fn render(self, session: &SessionInfo) {
        //let mut lines: Vec<Spans> = Vec::new();
        //let name_span = Span::raw(self.session_name);
        //let ids = ListItem::new(lines);
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Hash)]
pub struct ConnectInfoView {
    pub connect_id: String,
    pub addr: String,
    pub is_empty: bool,
    pub last_msg: String,
    pub last_status: String,
    pub state: String,
    pub msg_log: Vec<String>,
    pub parent: String,
}

impl ConnectInfoView {
    pub fn default() -> ConnectInfoView {
        let connect_id = String::new();
        let addr = String::new();
        let is_empty = true;
        let last_msg = String::new();
        let last_status = String::new();
        let state = String::new();
        let msg_log: Vec<String> = Vec::new();
        let parent = String::new();
        ConnectInfoView {
            connect_id,
            addr,
            is_empty,
            last_msg,
            last_status,
            state,
            msg_log,
            parent,
        }
    }

    pub fn update(mut self, data: ConnectInfo) -> Result<()> {
        self.connect_id = data.connect_id;
        self.addr = data.addr;
        self.is_empty = data.is_empty;
        self.last_msg = data.last_msg;
        self.last_status = data.last_status;
        self.state = data.state;
        self.msg_log = data.msg_log;
        self.parent = data.parent;
        Ok(())
    }

    pub fn render(self, connect: &ConnectInfo) {
        let mut lines: Vec<Spans> = Vec::new();
        //let name_span = Span::raw(self.connect_id);
    }
}

//#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Hash)]
//pub enum SelectableObject {
//    Node(NodeInfoView),
//    Session(SessionInfoView),
//    Connect(ConnectInfoViewView),
//}

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
pub struct InfoListView {
    pub index: usize,
    pub infos: FxHashMap<String, SelectableObject>,
}

impl InfoListView {
    pub fn new(infos: FxHashMap<String, SelectableObject>) -> InfoListView {
        let index = 0;

        InfoListView { index, infos }
    }

    pub async fn next(&mut self) {
        self.index = (self.index + 1) % self.infos.len();
    }

    pub async fn previous(&mut self) {
        if self.index > 0 {
            self.index -= 1;
        } else {
            self.index = self.infos.len() - 1;
        }
    }
}
