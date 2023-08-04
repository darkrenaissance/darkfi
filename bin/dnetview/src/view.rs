/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::collections::HashMap;

use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use darkfi::util::time::NanoTimestamp;

use crate::{
    error::{DnetViewError, DnetViewResult},
    model::{NodeInfo, SelectableObject},
};

type MsgLog = Vec<(NanoTimestamp, String, String)>;
type MsgMap = HashMap<String, MsgLog>;

#[derive(Debug, Clone)]
pub struct View {
    pub id_menu: IdMenu,
    pub msg_list: MsgList,
    pub selectables: HashMap<String, SelectableObject>,
    pub ordered_list: Vec<String>,
}

impl Default for View {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> View {
    pub fn new() -> Self {
        let msg_map = HashMap::new();
        let msg_list = MsgList::new(msg_map, 0);
        let selectables = HashMap::new();
        let id_menu = IdMenu::new(Vec::new());
        let ordered_list = Vec::new();

        Self { id_menu, msg_list, selectables, ordered_list }
    }

    pub fn update(&mut self, msg_map: MsgMap, selectables: HashMap<String, SelectableObject>) {
        self.update_selectable(selectables.clone());
        self.update_msg_list(msg_map);
        self.update_id_menu(selectables);
        self.update_msg_index();
        self.make_ordered_list();
    }

    fn update_id_menu(&mut self, selectables: HashMap<String, SelectableObject>) {
        for id in selectables.keys() {
            if !self.id_menu.ids.iter().any(|i| i == id) {
                self.id_menu.ids.push(id.to_string());
            }
        }
    }

    fn update_selectable(&mut self, selectables: HashMap<String, SelectableObject>) {
        for (id, obj) in selectables {
            self.selectables.insert(id, obj);
        }
    }

    fn make_ordered_list(&mut self) {
        for obj in self.selectables.values() {
            match obj {
                SelectableObject::Node(node) => {
                    if !self.ordered_list.iter().any(|i| i == &node.dnet_id) {
                        self.ordered_list.push(node.dnet_id.clone());
                    }
                    if !node.is_offline {
                        for session in &node.info {
                            if !session.is_empty {
                                if !self.ordered_list.iter().any(|i| i == &session.dnet_id) {
                                    self.ordered_list.push(session.dnet_id.clone());
                                }
                                for connection in &session.info {
                                    if !self.ordered_list.iter().any(|i| i == &connection.dnet_id) {
                                        self.ordered_list.push(connection.dnet_id.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                SelectableObject::Lilith(lilith) => {
                    if !self.ordered_list.iter().any(|i| i == &lilith.id) {
                        self.ordered_list.push(lilith.id.clone());
                    }
                    for network in &lilith.networks {
                        if !self.ordered_list.iter().any(|i| i == &network.id) {
                            self.ordered_list.push(network.id.clone());
                        }
                    }
                }
                _ => (),
            }
        }
        //debug!(target: "dnetview", "render_ids()::ordered_list: {:?}", self.ordered_list);
    }

    // TODO: this function is dynamically resizing the msgs index
    // according to what set of msgs is selected.
    // it's ugly. would prefer something more simple
    fn update_msg_index(&mut self) {
        if let Some(sel) = self.id_menu.state.selected() {
            if let Some(ord) = self.ordered_list.get(sel) {
                if let Some(i) = self.msg_list.msg_map.get(ord) {
                    self.msg_list.index = i.len();
                }
            }
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
        if self.ordered_list.is_empty() {
            // we have not received any data
            Ok(())
        } else {
            // get the id at the current index
            match self.id_menu.state.selected() {
                Some(i) => {
                    //debug!(target: "dnetview", "render()::selected index: {}", i);
                    match self.ordered_list.get(i) {
                        Some(i) => {
                            let id = i.clone();
                            self.render_info(f, slice, id)?;
                            Ok(())
                        }
                        None => Err(DnetViewError::NoIdAtIndex),
                    }
                }
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

        for obj in self.selectables.values() {
            match obj {
                SelectableObject::Node(node) => {
                    if node.is_offline {
                        let style = Style::default().fg(Color::Blue).add_modifier(Modifier::ITALIC);
                        let mut name = String::new();
                        name.push_str(&node.name);
                        name.push_str("(Offline)");
                        let name_span = Span::styled(name, style);
                        let lines = vec![Spans::from(name_span)];
                        let names = ListItem::new(lines);
                        nodes.push(names);
                    } else {
                        let name_span = Span::raw(&node.name);
                        let lines = vec![Spans::from(name_span)];
                        let names = ListItem::new(lines);
                        nodes.push(names);
                        for session in &node.info {
                            if !session.is_empty {
                                let name = Span::styled(format!("    {}", session.name), style);
                                let lines = vec![Spans::from(name)];
                                let names = ListItem::new(lines);
                                nodes.push(names);
                                for connection in &session.info {
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
                                                    addr, connection.remote_id
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
                SelectableObject::Lilith(lilith) => {
                    let name_span = Span::raw(&lilith.name);
                    let lines = vec![Spans::from(name_span)];
                    let names = ListItem::new(lines);
                    nodes.push(names);
                    for network in &lilith.networks {
                        let name = Span::styled(format!("    {}", network.name), style);
                        let lines = vec![Spans::from(name)];
                        let names = ListItem::new(lines);
                        nodes.push(names);
                    }
                }
                _ => (),
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
            //debug!(target: "dnetview", "render_info()::selected {}", selected);

            match info {
                Some(SelectableObject::Node(node)) => {
                    //debug!(target: "dnetview", "render_info()::SelectableObject::Node");
                    lines.push(Spans::from(Span::styled("Type: Normal", style)));
                    //match &node.external_addr {
                    //    Some(addr) => {
                    //        let node_info = Span::styled(format!("External addr: {}", addr), style);
                    //        lines.push(Spans::from(node_info));
                    //    }
                    //    None => {
                    //        let node_info = Span::styled("External addr: Null".to_string(), style);
                    //        lines.push(Spans::from(node_info));
                    //    }
                    //}
                    //lines.push(Spans::from(Span::styled(
                    //    format!("P2P state: {}", node.state),
                    //    style,
                    //)));
                }
                Some(SelectableObject::Session(session)) => {
                    //debug!(target: "dnetview", "render_info()::SelectableObject::Session");
                    //if session.addr.is_some() {
                    //    let accept_addr = Span::styled(
                    //        format!("Accept addr: {}", session.addr.as_ref().unwrap()),
                    //        style,
                    //    );
                    //    lines.push(Spans::from(accept_addr));
                    //}
                    //if session.hosts.is_some() {
                    //    let hosts = Span::styled("Hosts:".to_string(), style);
                    //    lines.push(Spans::from(hosts));
                    //    for host in session.hosts.as_ref().unwrap() {
                    //        let host = Span::styled(format!("      {}", host), style);
                    //        lines.push(Spans::from(host));
                    //    }
                    //}
                }
                Some(SelectableObject::Connect(connect)) => {
                    //debug!(target: "dnetview", "render_info()::SelectableObject::Connect");
                    let text = self.parse_msg_list(connect.dnet_id.clone())?;
                    f.render_stateful_widget(text, slice[1], &mut self.msg_list.state);
                }
                Some(SelectableObject::Lilith(_lilith)) => {
                    lines.push(Spans::from(Span::styled("Type: Lilith", style)));
                }
                Some(SelectableObject::Network(network)) => {
                    lines.push(Spans::from(Span::styled("URLs:", style)));
                    for url in &network.urls {
                        lines.push(Spans::from(Span::styled(format!("   {}", url), style)));
                    }
                    lines.push(Spans::from(Span::styled("Hosts:", style)));
                    for node in &network.nodes {
                        lines.push(Spans::from(Span::styled(format!("   {}", node), style)));
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
    pub infos: HashMap<String, NodeInfo>,
}

impl NodeInfoView {
    pub fn new(infos: HashMap<String, NodeInfo>) -> NodeInfoView {
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
