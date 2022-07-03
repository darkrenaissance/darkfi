use fxhash::FxHashMap;
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

//use log::debug;

type MsgLog = Vec<(NanoTimestamp, String, String)>;
type MsgMap = FxHashMap<String, MsgLog>;

#[derive(Debug)]
pub struct View {
    pub nodes: NodeInfoView,
    pub msg_list: MsgList,
    pub id_list: IdListView,
    pub selectables: FxHashMap<String, SelectableObject>,
}

impl<'a> View {
    pub fn new(
        nodes: NodeInfoView,
        msg_list: MsgList,
        id_list: IdListView,
        selectables: FxHashMap<String, SelectableObject>,
    ) -> View {
        View { nodes, msg_list, id_list, selectables }
    }

    pub fn update(
        &mut self,
        nodes: FxHashMap<String, NodeInfo>,
        msg_map: MsgMap,
        selectables: FxHashMap<String, SelectableObject>,
    ) {
        self.update_nodes(nodes);
        self.update_selectable(selectables);
        self.update_msg_list(msg_map);
        self.update_msg_len();
        self.update_ids();
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

    // get the msg_list at the selected connection ID
    // and set the list index to the size of the returned vector
    fn update_msg_len(&mut self) {
        match self.id_list.state.selected() {
            Some(i) => match self.id_list.ids.get(i) {
                Some(i) => match self.msg_list.msg_map.get(i) {
                    Some(i) => {
                        self.msg_list.msg_len = i.len();
                    }
                    None => {}
                },
                None => {}
            },
            None => {}
        }
    }

    fn update_msg_list(&mut self, msg_map: MsgMap) {
        for (id, msg) in msg_map {
            self.msg_list.msg_map.insert(id, msg);
        }
    }

    // step through all the data and update ids
    pub fn update_ids(&mut self) {
        self.id_list.ids.clear();
        for info in self.nodes.infos.values() {
            match info.is_offline {
                true => {
                    self.id_list.ids.push(info.id.clone());
                }
                false => {
                    self.id_list.ids.push(info.id.clone());
                    for session in &info.children {
                        if !session.is_empty {
                            self.id_list.ids.push(session.id.clone());
                            for connect in &session.children {
                                self.id_list.ids.push(connect.id.clone());
                            }
                        }
                    }
                }
            }
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

        self.render_ids(f, slice.clone())?;

        if !self.id_list.ids.is_empty() {
            // we have not received any data
            Ok(())
        } else {
            // get the id at the current index
            match self.id_list.state.selected() {
                Some(i) => match self.id_list.ids.get(i) {
                    Some(i) => {
                        let id = i.clone();
                        self.render_info(f, slice, id)?;
                        Ok(())
                    }
                    None => Err(DnetViewError::NoIdAtIndex),
                },
                // nothing is selected right now
                None => Ok(()),
            }
        }
    }

    fn render_ids<B: Backend>(
        &mut self,
        f: &mut Frame<'_, B>,
        slice: Vec<Rect>,
    ) -> DnetViewResult<()> {
        let style = Style::default();
        let mut nodes = Vec::new();

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
                }
                false => {
                    let name_span = Span::raw(&info.name);
                    let lines = vec![Spans::from(name_span)];
                    let names = ListItem::new(lines);
                    nodes.push(names);
                    for session in &info.children {
                        if !session.is_empty {
                            let name = Span::styled(format!("    {}", session.name), style);
                            let lines = vec![Spans::from(name)];
                            let names = ListItem::new(lines);
                            nodes.push(names);
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
                            }
                        }
                    }
                }
            }
        }
        let nodes =
            List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");

        f.render_stateful_widget(nodes, slice[0], &mut self.id_list.state);

        Ok(())
    }

    fn parse_msg_list(&self, connect: &ConnectInfo) -> DnetViewResult<List<'a>> {
        let send_style = Style::default().fg(Color::LightCyan);
        let recv_style = Style::default().fg(Color::DarkGray);
        let mut texts = Vec::new();
        let mut lines = Vec::new();
        let log = self.msg_list.msg_map.get(&connect.id);
        match log {
            Some(values) => {
                for (i, (t, k, v)) in values.iter().enumerate() {
                    lines.push(match k.as_str() {
                        "send" => {
                            Span::styled(format!("{}  {}             S: {}", i, t, v), send_style)
                        }
                        "recv" => {
                            Span::styled(format!("{}  {}             R: {}", i, t, v), recv_style)
                        }
                        data => return Err(DnetViewError::UnexpectedData(data.to_string())),
                    });
                }
            }
            None => return Err(DnetViewError::CannotFindId),
        }
        for line in lines.clone() {
            let text = ListItem::new(line);
            texts.push(text);
        }

        let msg_list = List::new(texts).block(Block::default().borders(Borders::ALL));

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
                            let node_info = Span::styled("External addr: Null".to_string(), style);
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
                    let text = self.parse_msg_list(connect)?;
                    f.render_stateful_widget(text, slice[1], &mut self.msg_list.state);
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
    pub ids: Vec<String>,
}

impl IdListView {
    pub fn new(ids: Vec<String>) -> IdListView {
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
    pub msg_map: MsgMap,
    pub msg_len: usize,
}

impl MsgList {
    pub fn new(msg_map: MsgMap, msg_len: usize) -> MsgList {
        MsgList { state: ListState::default(), msg_map, msg_len }
    }

    // TODO: reimplement
    //pub fn next(&mut self) {
    //    let i = match self.state.selected() {
    //        Some(i) => {
    //            if i >= self.msg_len - 1 {
    //                0
    //            } else {
    //                i + 1
    //            }
    //        }
    //        None => 0,
    //    };
    //    self.state.select(Some(i));
    //}

    //pub fn previous(&mut self) {
    //    let i = match self.state.selected() {
    //        Some(i) => {
    //            if i == 0 {
    //                self.msg_len - 1
    //            } else {
    //                i - 1
    //            }
    //        }
    //        None => 0,
    //    };
    //    self.state.select(Some(i));
    //}

    pub fn scroll(&mut self) -> DnetViewResult<()> {
        let i = match self.state.selected() {
            Some(i) => i + self.msg_len,
            None => 0,
        };
        self.state.select(Some(i));
        Ok(())
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
