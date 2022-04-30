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
    pub all_ids: IdListView,
    pub active_ids: IdListView,
    pub info_list: NodeInfoView,
    pub selectables: FxHashMap<String, SelectableObject>,
}

impl View {
    pub fn new(
        all_ids: IdListView,
        active_ids: IdListView,
        info_list: NodeInfoView,
        selectables: FxHashMap<String, SelectableObject>,
    ) -> View {
        View { all_ids, active_ids, info_list, selectables }
    }

    pub fn init_ids(&mut self, ids: FxHashSet<String>) {
        for id in ids {
            self.all_ids.ids.insert(id);
        }
    }

    pub fn init_node_info(&mut self, nodes: FxHashMap<String, NodeInfo>) {
        for (id, node) in nodes {
            self.info_list.infos.insert(id, node);
        }
    }

    pub fn init_selectable(&mut self, selectables: FxHashMap<String, SelectableObject>) {
        for (id, obj) in selectables {
            self.selectables.insert(id, obj);
        }
    }

    pub fn init_active_ids(&mut self) {
        for info in self.info_list.infos.values() {
            self.active_ids.ids.insert(info.node_id.to_string());
            for child in &info.children {
                if !child.is_empty == true {
                    self.active_ids.ids.insert(child.session_id.to_string());
                    for child in &child.children {
                        self.active_ids.ids.insert(child.connect_id.to_string());
                    }
                }
            }
        }
    }

    pub fn render<B: Backend>(mut self, f: &mut Frame<'_, B>) {
        let mut nodes = Vec::new();
        let style = Style::default();
        let list_margin = 2;
        let list_direction = Direction::Horizontal;
        let list_cnstrnts = vec![Constraint::Percentage(50), Constraint::Percentage(50)];

        for info in self.info_list.infos.values() {
            let name_span = Span::raw(&info.node_name);
            let lines = vec![Spans::from(name_span)];
            let names = ListItem::new(lines);
            nodes.push(names);
            for session in &info.children {
                if !session.is_empty == true {
                    let name = Span::styled(format!("    {}", session.session_name), style);
                    let lines = vec![Spans::from(name)];
                    let names = ListItem::new(lines);
                    nodes.push(names);
                    for connection in &session.children {
                        let mut info = Vec::new();
                        let name = Span::styled(format!("        {}", connection.addr), style);
                        info.push(name);
                        match connection.last_status.as_str() {
                            "recv" => {
                                let msg = Span::styled(
                                    format!("                    [R: {}]", connection.last_msg),
                                    style,
                                );
                                info.push(msg);
                            }
                            "sent" => {
                                let msg = Span::styled(
                                    format!("                    [S: {}]", connection.last_msg),
                                    style,
                                );
                                info.push(msg);
                            }
                            _ => {
                                // TODO
                            }
                        }

                        let lines = vec![Spans::from(info)];
                        let names = ListItem::new(lines);
                        nodes.push(names);
                    }
                }
            }
        }

        let slice = Layout::default()
            .direction(list_direction)
            .margin(list_margin)
            .constraints(list_cnstrnts)
            .split(f.size());

        //debug!("NODE INFO LENGTH {:?}", nodes.len());
        //debug!("ACTIVE IDS LENGTH {:?}", self.active_ids.ids.len());

        let nodes =
            List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");

        f.render_stateful_widget(nodes, slice[0], &mut self.active_ids.state);

        // TODO: render another stateful widget that shares the same state
        // but displays SelectableObject on the right
        self.render_info(f, slice);
    }

    fn render_info<B: Backend>(self, f: &mut Frame<'_, B>, slice: Vec<Rect>) {
        let span = vec![];
        let graph = Paragraph::new(span)
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default());

        for info in self.selectables.values() {
            match info {
                SelectableObject::Node(node) => {}
                SelectableObject::Session(session) => {}
                SelectableObject::Connect(connect) => {}
            }
            //
        }

        //f.render_stateful_widget(nodes, slice[0], &mut self.active_ids.state);
        f.render_widget(graph, slice[1]);
    }
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
                if i >= self.ids.len() - 1 {
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
                if i == 0 {
                    self.ids.len() - 1
                } else {
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
