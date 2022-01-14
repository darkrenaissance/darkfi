// first one selected

// list on left
// info page on right
// when selected takes up full page
// identifier is random string
//
// event handler for the list
// when the list is selected
// updates a hashmap that keep track of selected??

use crate::app::App;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Spans,
    widgets::{Block, List, ListItem, Paragraph},
    Frame,
};

pub fn ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let slice = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(f.size());

    let nodes: Vec<ListItem> = app
        .node_list
        .nodes
        .iter()
        .map(|(id, info)| {
            let line1 = Spans::from(id.to_string());
            let line2 = Spans::from(info.to_string());

            ListItem::new(vec![line1, line2]).style(Style::default())
        })
        .collect();

    let nodes = List::new(nodes)
        .block(Block::default())
        .highlight_style(Style::default().bg(Color::Red).add_modifier(Modifier::BOLD));

    f.render_stateful_widget(nodes, slice[0], &mut app.node_list.state);

    let node_info: Vec<ListItem> = app
        .node_list
        .node_info
        .noise
        .iter()
        .map(|i| {
            let line1 = Spans::from(i.to_string());
            ListItem::new(vec![line1]).style(Style::default())
        })
        .collect();

    let node_info = List::new(node_info)
        .block(Block::default())
        .highlight_style(Style::default().bg(Color::Blue).add_modifier(Modifier::BOLD));

    // TODO: make this a stateful widget that changes with scroll
    f.render_stateful_widget(node_info, slice[1], &mut app.node_list.state);
}
