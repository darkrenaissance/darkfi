use crate::{model::NodeInfo, view::View};
use log::debug;
use std::collections::HashMap;

use tui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
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
            let mut lines = vec![Spans::from(id.to_string())];
            lines.push(Spans::from(""));
            lines.push(Spans::from(""));
            lines.push(Spans::from(""));
            lines.push(Spans::from(""));
            lines.push(Spans::from(""));
            lines.push(Spans::from(""));
            ListItem::new(lines).style(Style::default())
        })
        .collect();

    let nodes =
        List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");

    f.render_stateful_widget(nodes, slice[0], &mut view.id_list.state);

    let index = view.info_list.index;

    render_info_left(view.clone(), f, index, slice.clone());
    render_info_right(view.clone(), f, index, slice.clone());
}

fn render_info_left<B: Backend>(view: View, f: &mut Frame<'_, B>, _index: usize, slice: Vec<Rect>) {
    let slice = Layout::default()
        .direction(Direction::Horizontal)
        .horizontal_margin(8)
        .vertical_margin(4)
        .constraints([Constraint::Percentage(100)].as_ref())
        .split(f.size());

    let info = &view.info_list.infos;

    let mut titles = Vec::new();
    for id in &view.id_list.node_id {
        match info.get(id) {
            Some(_) => {
                titles.push(Spans::from(Span::styled("Outgoing:", Style::default())));
                titles.push(Spans::from(""));
                titles.push(Spans::from(""));
                titles.push(Spans::from(Span::styled("Incoming:", Style::default())));
                titles.push(Spans::from(""));
                titles.push(Spans::from(""));
                titles.push(Spans::from(""));
            }
            None => {
                // TODO
            }
        }
    }
    let title_graph = Paragraph::new(titles).style(Style::default()).alignment(Alignment::Left);

    f.render_widget(title_graph, slice[0]);

    let slice = Layout::default()
        .direction(Direction::Horizontal)
        .horizontal_margin(6)
        .vertical_margin(4)
        .constraints([Constraint::Percentage(47), Constraint::Percentage(53)].as_ref())
        .split(f.size());

    let mut msgs = Vec::new();

    for id in &view.id_list.node_id {
        match info.get(id) {
            Some(connects) => {
                msgs.push(Spans::from(""));
                msgs.push(Spans::from(format!("[R: {}]", connects.outgoing[0].message)));
                msgs.push(Spans::from(format!("[S: {}]", connects.outgoing[1].message)));
                msgs.push(Spans::from(""));
                msgs.push(Spans::from(format!("[R: {}]", connects.incoming[0].message)));
                msgs.push(Spans::from(format!("[S: {}]", connects.incoming[1].message)));
                msgs.push(Spans::from(""));
            }
            None => {
                // TODO
            }
        }
    }

    let msg_graph = Paragraph::new(msgs).style(Style::default()).alignment(Alignment::Right);

    f.render_widget(msg_graph, slice[0]);

    let slice = Layout::default()
        .direction(Direction::Horizontal)
        .horizontal_margin(10)
        .vertical_margin(4)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)].as_ref())
        .split(f.size());

    let mut ids = Vec::new();

    for id in &view.id_list.node_id {
        match info.get(id) {
            Some(connects) => {
                ids.push(Spans::from(""));
                ids.push(Spans::from(format!("{}", connects.outgoing[0].id)));
                ids.push(Spans::from(format!("{}", connects.outgoing[1].id)));
                ids.push(Spans::from(""));
                ids.push(Spans::from(format!("{}", connects.incoming[0].id)));
                ids.push(Spans::from(format!("{}", connects.incoming[1].id)));
                ids.push(Spans::from(""));
            }
            None => {
                // TODO
            }
        }
    }

    let id_graph = Paragraph::new(ids).style(Style::default()).alignment(Alignment::Left);

    f.render_widget(id_graph, slice[0]);
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
