//use darkfi::error::{Error, Result};
use fxhash::{FxHashMap, FxHashSet};
use tui::widgets::ListState;

use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::{
    error::{DnetViewError, DnetViewResult},
    model::{NodeInfo, SelectableObject},
};
use log::debug;

#[derive(Debug)]
pub struct View {
    pub nodes: NodeInfoView,
    pub msg_log: FxHashMap<String, Vec<(u64, String, String)>>,
    pub active_ids: IdListView,
    pub selectables: FxHashMap<String, SelectableObject>,
}

impl View {
    pub fn new(
        nodes: NodeInfoView,
        msg_log: FxHashMap<String, Vec<(u64, String, String)>>,
        active_ids: IdListView,
        selectables: FxHashMap<String, SelectableObject>,
    ) -> View {
        View { nodes, msg_log, active_ids, selectables }
    }

    pub fn update(
        &mut self,
        nodes: FxHashMap<String, NodeInfo>,
        msg_log: FxHashMap<String, Vec<(u64, String, String)>>,
        selectables: FxHashMap<String, SelectableObject>,
    ) {
        self.update_nodes(nodes);
        self.update_selectable(selectables);
        self.update_active_ids();
        self.update_msg_log(msg_log);
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
        for info in self.nodes.infos.values() {
            self.active_ids.ids.insert(info.id.to_string());
            for child in &info.children {
                if !child.is_empty == true {
                    self.active_ids.ids.insert(child.id.to_string());
                    for child in &child.children {
                        self.active_ids.ids.insert(child.id.to_string());
                    }
                }
            }
        }
    }

    fn update_msg_log(&mut self, msg_log: FxHashMap<String, Vec<(u64, String, String)>>) {
        for (id, msg) in msg_log {
            self.msg_log.insert(id, msg);
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

        // remove any duplicates
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
                    None => return Err(DnetViewError::NoIdAtIndex),
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
        let mut ids: Vec<String> = Vec::new();

        for info in self.nodes.infos.values() {
            let name_span = Span::raw(&info.name);
            let lines = vec![Spans::from(name_span)];
            let names = ListItem::new(lines);
            nodes.push(names);
            ids.push(info.id.clone());
            for session in &info.children {
                if !session.is_empty == true {
                    let name = Span::styled(format!("    {}", session.name), style);
                    let lines = vec![Spans::from(name)];
                    let names = ListItem::new(lines);
                    nodes.push(names);
                    ids.push(session.id.clone());
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
                            "Null" => {
                                // Empty msg log. Do nothing
                            }
                            data => return Err(DnetViewError::UnexpectedData(data.to_string())),
                        }

                        let lines = vec![Spans::from(info)];
                        let names = ListItem::new(lines);
                        nodes.push(names);
                        ids.push(connection.id.clone());
                    }
                }
            }
        }

        let nodes =
            List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");

        f.render_stateful_widget(nodes, slice[0], &mut self.active_ids.state);

        Ok(ids)
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
                    let node_info =
                        Span::styled(format!("External addr: {}", node.external_addr), style);
                    lines.push(Spans::from(node_info));
                }
                Some(SelectableObject::Session(_session)) => {
                    //let name_span = Spans::from("Session Info");
                    //spans.push(name_span);
                }
                Some(SelectableObject::Connect(connect)) => {
                    let log = self.msg_log.get(&connect.id);
                    match log {
                        Some(values) => {
                            for (t, k, v) in values {
                                lines.push(Spans::from(match k.as_str() {
                                    "send" => Span::styled(format!("S: {}", v), style),
                                    "recv" => Span::styled(format!("R: {}", v), style),
                                    data => {
                                        return Err(DnetViewError::UnexpectedData(data.to_string()))
                                    }
                                }));
                            }
                        }
                        None => return Err(DnetViewError::CannotFindId),
                    }
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
