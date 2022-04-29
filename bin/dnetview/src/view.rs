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

    pub fn init_active_ids(&mut self) {
        for (id, node) in &self.info_list.infos {
            self.active_ids.ids.insert(id.to_string());
            for session in &node.children {
                if session.is_empty == false {
                    self.active_ids.ids.insert(session.session_id.to_string());
                    for connection in &session.children {
                        self.active_ids.ids.insert(connection.connect_id.to_string());
                    }
                }
            }
        }
        //debug!("ACTIVE IDS {:?}", self.active_ids.ids);
        //debug!("ALL IDS {:?}", self.all_ids.ids);
    }

    pub fn init_ids(&mut self, ids: FxHashSet<String>) {
        for id in ids {
            self.all_ids.ids.insert(id);
        }
        self.init_active_ids();
    }

    pub fn init_node_info(&mut self, nodes: FxHashMap<String, NodeInfo>) {
        for (id, node) in nodes {
            self.info_list.infos.insert(id, node);
        }
    }

    pub fn init_selectable(&mut self, selectables: FxHashMap<String, SelectableObject>) {
        // TODO: remove unactive selectables
        for (id, obj) in selectables {
            self.selectables.insert(id, obj);
        }
    }

    pub fn render<B: Backend>(mut self, f: &mut Frame<'_, B>) {
        //debug!("VIEW AT RENDER {:?}", self.id_list.ids);
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
            for child in &info.children {
                let name = Span::styled(format!("    {}", child.session_name), style);
                let lines = vec![Spans::from(name)];
                let names = ListItem::new(lines);
                nodes.push(names);
                for child in &child.children {
                    let name = Span::styled(format!("        {}", child.addr), style);
                    let lines = vec![Spans::from(name)];
                    let names = ListItem::new(lines);
                    nodes.push(names);
                }
            }
        }

        let slice = Layout::default()
            .direction(list_direction)
            .margin(list_margin)
            .constraints(list_cnstrnts)
            .split(f.size());

        let nodes =
            List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");

        f.render_stateful_widget(nodes, slice[0], &mut self.active_ids.state);
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
