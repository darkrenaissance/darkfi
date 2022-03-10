use crate::view::View;
//use log::debug;

use tui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

pub fn ui<B: Backend>(f: &mut Frame<'_, B>, mut view: View) {
    let slice = Layout::default()
        .direction(Direction::Horizontal)
        .margin(2)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(f.size());

    let nodes: Vec<ListItem> = view
        .id_list
        .node_id
        .iter()
        .map(|id| {
            let lines = vec![Spans::from(id.to_string())];
            ListItem::new(lines).style(Style::default())
        })
        .collect();

    let nodes =
        List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");

    f.render_stateful_widget(nodes, slice[0], &mut view.id_list.state);

    let index = view.info_list.index;

    render_info_left(view.clone(), f);
    render_info_right(view.clone(), f, index, slice);
}

fn render_info_left<B: Backend>(view: View, f: &mut Frame<'_, B>) {
    let length = render_outbound(view.clone(), f);
    let length = render_inbound(view.clone(), f, length);
    render_manual(view.clone(), f, length);
}

fn render_manual<B: Backend>(view: View, f: &mut Frame<'_, B>, length: usize) {
    let new_num: u16 = length.try_into().unwrap();
    let title_slice = Layout::default()
        .direction(Direction::Horizontal)
        .horizontal_margin(8)
        .vertical_margin(new_num)
        .constraints([Constraint::Percentage(100)].as_ref())
        .split(f.size());

    let info_slice = Layout::default()
        .direction(Direction::Horizontal)
        .horizontal_margin(10)
        .vertical_margin(new_num)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)].as_ref())
        .split(f.size());

    let info = &view.info_list.infos;
    let mut title = Vec::new();
    for id in &view.id_list.node_id {
        match info.get(id) {
            Some(_) => {
                title.push(Spans::from(Span::styled("Manual:", Style::default())));
            }
            None => {
                // TODO
            }
        }
    }

    let mut man_info = Vec::new();
    for id in &view.id_list.node_id {
        match info.get(id) {
            Some(connects) => {
                man_info.push(Spans::from(""));
                man_info.push(Spans::from(format!("Key: {}", connects.manual[0].key)));
                man_info.push(Spans::from(""));
            }
            None => {
                // TODO
            }
        }
    }

    let info_graph = Paragraph::new(man_info).style(Style::default()).alignment(Alignment::Left);
    let title_graph = Paragraph::new(title).style(Style::default()).alignment(Alignment::Left);

    f.render_widget(info_graph, info_slice[0]);
    f.render_widget(title_graph, title_slice[0]);
}
fn render_inbound<B: Backend>(view: View, f: &mut Frame<'_, B>, length: usize) -> usize {
    let mut inbound_info = Vec::new();
    // TODO: find better way of doing this
    let new_num: u16 = length.try_into().unwrap();
    let num = new_num + 3;
    let title_slice = Layout::default()
        .direction(Direction::Horizontal)
        .horizontal_margin(8)
        .vertical_margin(num)
        .constraints([Constraint::Percentage(100)].as_ref())
        .split(f.size());

    let info_slice = Layout::default()
        .direction(Direction::Horizontal)
        .horizontal_margin(10)
        .vertical_margin(num)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)].as_ref())
        .split(f.size());

    let info = &view.info_list.infos;
    let mut title = Vec::new();
    for id in &view.id_list.node_id {
        match info.get(id) {
            Some(_) => {
                title.push(Spans::from(Span::styled("Inbound:", Style::default())));
            }
            None => {
                // TODO
            }
        }
    }

    for id in &view.id_list.node_id {
        match info.get(id) {
            Some(connects) => {
                if connects.inbound.is_empty() {
                    inbound_info.push(Spans::from(""));
                    inbound_info.push(Spans::from(format!("Connected: Null")));
                    inbound_info.push(Spans::from(format!("Last msg: Null")));
                    inbound_info.push(Spans::from(format!("Last status: Null")));
                } else {
                    for connect in &connects.inbound {
                        inbound_info.push(Spans::from(""));
                        inbound_info.push(Spans::from(format!("Connected: {}", connect.connected)));
                        inbound_info
                            .push(Spans::from(format!("Last msg: {}", connect.channel.last_msg)));
                        inbound_info.push(Spans::from(format!(
                            "Last status: {}",
                            connect.channel.last_status
                        )));
                    }
                }
            }
            None => {
                // TODO
            }
        }
    }
    for _n in 1..info.len() {
        inbound_info.push(Spans::from(""))
    }

    let info_graph =
        Paragraph::new(inbound_info.clone()).style(Style::default()).alignment(Alignment::Left);
    let title_graph =
        Paragraph::new(title.clone()).style(Style::default()).alignment(Alignment::Left);

    f.render_widget(info_graph, info_slice[0]);
    f.render_widget(title_graph, title_slice[0]);

    return inbound_info.len() + title.len() + length + 2
}

fn render_outbound<B: Backend>(view: View, f: &mut Frame<'_, B>) -> usize {
    let title_slice = Layout::default()
        .direction(Direction::Horizontal)
        .horizontal_margin(8)
        .vertical_margin(4)
        .constraints([Constraint::Percentage(100)].as_ref())
        .split(f.size());

    let info_slice = Layout::default()
        .direction(Direction::Horizontal)
        .horizontal_margin(10)
        .vertical_margin(4)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)].as_ref())
        .split(f.size());

    let info = &view.info_list.infos;
    let mut title = Vec::new();
    for id in &view.id_list.node_id {
        match info.get(id) {
            Some(_) => {
                title.push(Spans::from(Span::styled("Outbound:", Style::default())));
            }
            None => {
                // TODO
            }
        }
    }

    let mut slots = Vec::new();
    for id in &view.id_list.node_id {
        match info.get(id) {
            Some(connects) => {
                for slot in &connects.outbound[0].slots {
                    slots.push(Spans::from(""));
                    slots.push(Spans::from(format!("Addr: {}", slot.addr)));
                    slots.push(Spans::from(format!("Last msg: {}", slot.channel.last_msg)));
                    slots.push(Spans::from(format!("Last status: {}", slot.channel.last_status)));
                }
            }
            None => {
                // TODO
            }
        }
    }
    for _n in 1..info.len() {
        slots.push(Spans::from(""))
    }

    let info_graph =
        Paragraph::new(slots.clone()).style(Style::default()).alignment(Alignment::Left);
    let title_graph =
        Paragraph::new(title.clone()).style(Style::default()).alignment(Alignment::Left);

    f.render_widget(info_graph, info_slice[0]);
    f.render_widget(title_graph, title_slice[0]);

    let out_len = slots.len() + title.len();
    return out_len
}

fn render_info_right<B: Backend>(
    _view: View,
    f: &mut Frame<'_, B>,
    _index: usize,
    slice: Vec<Rect>,
) {
    let span = vec![];
    let graph =
        Paragraph::new(span).block(Block::default().borders(Borders::ALL)).style(Style::default());
    f.render_widget(graph, slice[1]);
}
