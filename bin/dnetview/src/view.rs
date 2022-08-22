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
    model::{NodeInfo, SelectableObject},
};

use log::debug;

type MsgLog = Vec<(NanoTimestamp, String, String)>;
type MsgMap = FxHashMap<String, MsgLog>;

#[derive(Debug, Clone)]
pub struct View {
    pub id_menu: IdMenu,
    pub msg_list: MsgList,
    pub selectables: FxHashMap<String, SelectableObject>,
}

impl<'a> View {
    pub fn new() -> View {
        let msg_map = FxHashMap::default();
        let msg_list = MsgList::new(msg_map.clone(), 0);
        let selectables = FxHashMap::default();
        let id_menu = IdMenu::new(Vec::new());

        View { id_menu, msg_list, selectables }
    }

    pub fn update(&mut self, msg_map: MsgMap, selectables: FxHashMap<String, SelectableObject>) {
        self.update_selectable(selectables.clone());
        self.update_msg_list(msg_map);
        self.update_id_menu(selectables);
        self.update_msg_index();
    }

    fn update_id_menu(&mut self, selectables: FxHashMap<String, SelectableObject>) {
        for id in selectables.keys() {
            if !self.id_menu.ids.iter().any(|i| i == id) {
                self.id_menu.ids.push(id.to_string());
            }
        }
    }

    fn update_selectable(&mut self, selectables: FxHashMap<String, SelectableObject>) {
        for (id, obj) in selectables {
            self.selectables.insert(id, obj);
        }
    }

    // TODO: this function is dynamically resizing the msgs index
    // according to what set of msgs is selected.
    // it's ugly. would prefer something more simple
    fn update_msg_index(&mut self) {
        match self.id_menu.state.selected() {
            Some(i) => match self.id_menu.ids.get(i) {
                Some(i) => match self.msg_list.msg_map.get(i) {
                    Some(i) => {
                        self.msg_list.index = i.len();
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
        if self.id_menu.ids.is_empty() {
            // we have not received any data
            Ok(())
        } else {
            // get the id at the current index
            match self.id_menu.state.selected() {
                Some(i) => match self.id_menu.ids.get(i) {
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

    // either the Vec<String> needs to be reordered according to the order of nodes: Vec<Spans>
    // or Vec<Spans> needs to be reordered according to Vec<String>
    fn render_ids<B: Backend>(
        &mut self,
        f: &mut Frame<'_, B>,
        slice: Vec<Rect>,
    ) -> DnetViewResult<()> {
        let style = Style::default();
        let mut nodes = Vec::new();

        for obj in self.selectables.values() {
            match obj {
                SelectableObject::Node(info) => match info.is_offline {
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
                },
                _ => {}
            }
        }
        let nodes =
            List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");

        f.render_stateful_widget(nodes, slice[0], &mut self.id_menu.state);

        Ok(())
    }

    fn parse_msg_list(&self, connect_id: String) -> DnetViewResult<List<'a>> {
        let send_style = Style::default().fg(Color::LightCyan);
        let recv_style = Style::default().fg(Color::DarkGray);
        let mut texts = Vec::new();
        let mut lines = Vec::new();
        let log = self.msg_list.msg_map.get(&connect_id);
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
            debug!(target: "dnetview", "render_info()::selected {}", selected);

            match info {
                Some(SelectableObject::Node(node)) => {
                    debug!(target: "dnetview", "render_info()::SelectableObject::Node");
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
                    debug!(target: "dnetview", "render_info()::SelectableObject::Session");
                    if session.accept_addr.is_some() {
                        let session_info = Span::styled(
                            format!("Accept addr: {}", session.accept_addr.as_ref().unwrap()),
                            style,
                        );
                        lines.push(Spans::from(session_info));
                    }
                }
                Some(SelectableObject::Connect(connect)) => {
                    debug!(target: "dnetview", "render_info()::SelectableObject::Connect");
                    let text = self.parse_msg_list(connect.id.clone())?;
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
pub struct IdMenu {
    pub state: ListState,
    pub ids: Vec<String>,
}

impl IdMenu {
    pub fn new(ids: Vec<String>) -> IdMenu {
        IdMenu { state: ListState::default(), ids }
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
    pub index: usize,
}

impl MsgList {
    pub fn new(msg_map: MsgMap, index: usize) -> MsgList {
        MsgList { state: ListState::default(), msg_map, index }
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
            Some(i) => i + self.index,
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

    //pub fn next(&mut self) {
    //    self.index = (self.index + 1) % self.infos.len();
    //}

    //pub fn previous(&mut self) {
    //    if self.index > 0 {
    //        self.index -= 1;
    //    } else {
    //        self.index = self.infos.len() - 1;
    //    }
    //}
}
