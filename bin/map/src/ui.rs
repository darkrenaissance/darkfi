use crate::model::Model;
use async_std::sync::{Arc, Mutex};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Spans,
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

pub fn ui<B: Backend>(f: &mut Frame<'_, B>, mut app: Model) {
    let slice = Layout::default()
        .direction(Direction::Horizontal)
        .margin(2)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(f.size());

    let nodes: Vec<ListItem> = app
        .id_list
        .node_id
        .iter()
        .map(|id| {
            let line1 = Spans::from(id.to_string());
            ListItem::new(vec![line1]).style(Style::default())
        })
        .collect();

    let nodes = List::new(nodes)
        .block(Block::default().borders(Borders::ALL))
        .highlight_style(Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD));

    // needs to be mutable. could
    f.render_stateful_widget(nodes, slice[0], &mut app.id_list.state);

    let index = app.info_list.index;

    render_info(app, f, index, slice);
}

fn render_info<B: Backend>(app: Model, f: &mut Frame<'_, B>, index: usize, slice: Vec<Rect>) {
    let info = &app.info_list.infos;
    let id = &info[index].id;
    let connections = info[index].connections;
    let is_active = info[index].is_active;
    let message = &info[index].last_message;
    let span = vec![
        Spans::from(format!("NodeId: {}", id)),
        Spans::from(format!("Number of connections: {}", connections)),
        Spans::from(format!("Is active: {}", is_active)),
        Spans::from(format!("Last message: {}", message)),
    ];
    let graph =
        Paragraph::new(span).block(Block::default().borders(Borders::ALL)).style(Style::default());
    f.render_widget(graph, slice[1]);
}
