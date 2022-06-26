//use darkfi::error::{Error, Result};
use fxhash::{FxHashMap, FxHashSet};
use tui::widgets::ListState;

use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use darkfi::util::NanoTimestamp;

use crate::{
    error::{DnetViewError, DnetViewResult},
    model::{ConnectInfo, NodeInfo, SelectableObject},
};

use log::debug;

#[derive(Debug)]
pub struct View {
    pub nodes: NodeInfoView,
    pub msg_list: MsgList,
    //pub msg_log: FxHashMap<String, Vec<(NanoTimestamp, String, String)>>,
    pub active_ids: IdListView,
    pub selectables: FxHashMap<String, SelectableObject>,
}

impl<'a> View {
    pub fn new(
        nodes: NodeInfoView,
        msg_list: MsgList,
        active_ids: IdListView,
        selectables: FxHashMap<String, SelectableObject>,
    ) -> View {
        View { nodes, msg_list, active_ids, selectables }
    }

    pub fn update(
        &mut self,
        nodes: FxHashMap<String, NodeInfo>,
        msg_list: FxHashMap<String, Vec<(NanoTimestamp, String, String)>>,
        selectables: FxHashMap<String, SelectableObject>,
    ) {
        self.update_nodes(nodes);
        self.update_selectable(selectables);
        self.update_active_ids();
        self.update_msg_log(msg_list);
    }

    fn update_nodes(&mut self, nodes: FxHashMap<String, NodeInfo>) {
        for (id, node) in nodes {
            self.nodes.infos.insert(id, node);
        }
    }

    fn update_selectable(&mut self, selectables: FxHashMap<String, SelectableObject>) {
        for (id, obj) in selectables {
            self.selectables.insert(id, obj);
        }
    }

    fn update_active_ids(&mut self) {
        self.active_ids.ids.clear();
        for info in self.nodes.infos.values() {
            self.active_ids.ids.insert(info.id.to_string());
            for session in &info.children {
                if !session.is_empty {
                    self.active_ids.ids.insert(session.id.to_string());
                    for connect in &session.children {
                        self.active_ids.ids.insert(connect.id.to_string());
                    }
                }
            }
        }
    }

    fn update_msg_log(&mut self, msg_log: FxHashMap<String, Vec<(NanoTimestamp, String, String)>>) {
        for (id, msg) in msg_log {
            self.msg_list.msg_log.insert(id, msg);
        }
    }

    pub fn render<B: Backend>(&mut self, f: &mut Frame<'_, B>) -> DnetViewResult<()> {
        let margin = 2;
        let direction = Direction::Horizontal;
        let cnstrnts = vec![Constraint::Percentage(50), Constraint::Percentage(50)];

        let slice = Layout::default()
            .direction(direction)
            .margin(margin)
            .constraints(cnstrnts)
            .split(f.size());

        let mut id_list = self.render_id_list(f, slice.clone())?;

        // might not need this
        // also assert in test case might be better
        id_list.dedup();
        if id_list.is_empty() {
            // we have not received any data
            Ok(())
        } else {
            // get the id at the current index
            match self.active_ids.state.selected() {
                Some(i) => match id_list.get(i) {
                    Some(i) => {
                        self.render_info(f, slice.clone(), i.to_string())?;
                        Ok(())
                    }
                    None => Err(DnetViewError::NoIdAtIndex),
                },
                // nothing is selected right now
                None => Ok(()),
            }
        }
    }

    fn render_id_list<B: Backend>(
        &mut self,
        f: &mut Frame<'_, B>,
        slice: Vec<Rect>,
    ) -> DnetViewResult<Vec<String>> {
        let style = Style::default();
        let mut nodes = Vec::new();
        let mut node_ids: Vec<String> = Vec::new();
        let mut session_ids: Vec<String> = Vec::new();
        let mut connect_ids: Vec<String> = Vec::new();
        let mut ids: Vec<String> = Vec::new();

        for info in self.nodes.infos.values() {
            match info.is_offline {
                true => {
                    let style = Style::default().fg(Color::Blue).add_modifier(Modifier::ITALIC);
                    let mut name = String::new();
                    name.push_str(&info.name);
                    name.push_str("(Offline)");
                    let name_span = Span::styled(name, style);
                    let lines = vec![Spans::from(name_span)];
                    let names = ListItem::new(lines);
                    nodes.push(names);
                    ids.push(info.id.clone());
                    node_ids.push(info.id.clone());
                }
                false => {
                    let name_span = Span::raw(&info.name);
                    let lines = vec![Spans::from(name_span)];
                    let names = ListItem::new(lines);
                    nodes.push(names);
                    ids.push(info.id.clone());
                    node_ids.push(info.id.clone());
                    for session in &info.children {
                        if !session.is_empty {
                            let name = Span::styled(format!("    {}", session.name), style);
                            let lines = vec![Spans::from(name)];
                            let names = ListItem::new(lines);
                            nodes.push(names);
                            session_ids.push(session.id.clone());
                            ids.push(session.id.clone());
                            for connection in &session.children {
                                let mut info = Vec::new();
                                match connection.addr.as_str() {
                                    "Null" => {
                                        let style = Style::default()
                                            .fg(Color::Blue)
                                            .add_modifier(Modifier::ITALIC);
                                        let name = Span::styled(
                                            format!("        {} ", connection.addr),
                                            style,
                                        );
                                        info.push(name);
                                    }
                                    addr => {
                                        let name = Span::styled(
                                            format!(
                                                "        {} ({})",
                                                addr, connection.remote_node_id
                                            ),
                                            style,
                                        );
                                        info.push(name);
                                    }
                                }

                                let lines = vec![Spans::from(info)];
                                let names = ListItem::new(lines);
                                nodes.push(names);
                                connect_ids.push(connection.id.clone());
                                ids.push(connection.id.clone());
                            }
                        }
                    }
                }
            }
        }
        let nodes =
            List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");

        f.render_stateful_widget(nodes, slice[0], &mut self.active_ids.state);

        Ok(ids)
    }

    fn parse_msg_list<'_a>(&self, connect: &ConnectInfo) -> DnetViewResult<List<'a>> {
        let style = Style::default();
        let mut list_vec = Vec::new();
        let mut lines = Vec::new();
        let log = self.msg_list.msg_log.get(&connect.id);
        match log {
            Some(values) => {
                for (i, (t, k, v)) in values.into_iter().enumerate() {
                    lines.push(Span::from(match k.as_str() {
                        "send" => Span::styled(format!("{}  {}             S: {}", i, t, v), style),
                        "recv" => Span::styled(format!("{}  {}             R: {}", i, t, v), style),
                        data => return Err(DnetViewError::UnexpectedData(data.to_string())),
                    }));
                }
            }
            None => return Err(DnetViewError::CannotFindId),
        }
        for line in lines.clone() {
            let list = ListItem::new(line);
            list_vec.push(list);
        }

        let msg_list = List::new(list_vec)
            .block(Block::default().borders(Borders::ALL))
            .highlight_symbol(">> ");

        Ok(msg_list)
    }

    fn render_info<B: Backend>(
        &mut self,
        f: &mut Frame<'_, B>,
        slice: Vec<Rect>,
        selected: String,
    ) -> DnetViewResult<()> {
        let style = Style::default();
        let mut lines = Vec::new();

        if self.selectables.is_empty() {
            // we have not received any selectable data
            return Ok(())
        } else {
            let info = self.selectables.get(&selected);

            match info {
                Some(SelectableObject::Node(node)) => {
                    match &node.external_addr {
                        Some(addr) => {
                            let node_info = Span::styled(format!("External addr: {}", addr), style);
                            lines.push(Spans::from(node_info));
                        }
                        None => {
                            let node_info = Span::styled(format!("External addr: Null"), style);
                            lines.push(Spans::from(node_info));
                        }
                    }
                    lines.push(Spans::from(Span::styled(
                        format!("P2P state: {}", node.state),
                        style,
                    )));
                }
                Some(SelectableObject::Session(session)) => {
                    if session.accept_addr.is_some() {
                        let session_info = Span::styled(
                            format!("Accept addr: {}", session.accept_addr.as_ref().unwrap()),
                            style,
                        );
                        lines.push(Spans::from(session_info));
                    }
                }
                Some(SelectableObject::Connect(connect)) => {
                    let list = self.parse_msg_list(connect)?;
                    f.render_stateful_widget(list, slice[1], &mut self.active_ids.state);
                }
                None => return Err(DnetViewError::NotSelectableObject),
            }
        }

        let graph = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default());

        f.render_widget(graph, slice[1]);

        Ok(())
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
pub struct MsgList {
    pub state: ListState,
    pub msg_log: FxHashMap<String, Vec<(NanoTimestamp, String, String)>>,
}

impl MsgList {
    pub fn new(msg_log: FxHashMap<String, Vec<(NanoTimestamp, String, String)>>) -> MsgList {
        MsgList { state: ListState::default(), msg_log }
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.msg_log.len() - 1 {
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
                    self.msg_log.len() - 1
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
