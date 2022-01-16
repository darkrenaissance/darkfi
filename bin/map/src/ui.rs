use crate::app::App;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Spans,
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

pub fn ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let slice = Layout::default()
        .direction(Direction::Horizontal)
        .margin(2)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(f.size());

    let nodes: Vec<ListItem> = app
        .node_list
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

    f.render_stateful_widget(nodes, slice[0], &mut app.node_list.state);

    // TODO: cleanup this boilerplate
    // pass index value to render_info()
    match app.node_info.index {
        0 => {
            let id = &app.node_info.infos[0].id;
            let connections = app.node_info.infos[0].connections;
            let span = vec![
                Spans::from(format!("NodeId: {}", id)),
                Spans::from(format!("Number of connections: {}", connections)),
            ];
            let graph = Paragraph::new(span)
                .block(Block::default().borders(Borders::ALL))
                .style(Style::default());
            f.render_widget(graph, slice[1]);
        }
        1 => {
            let id = &app.node_info.infos[1].id;
            let connections = app.node_info.infos[1].connections;
            let span = vec![
                Spans::from(format!("NodeId: {}", id)),
                Spans::from(format!("Number of connections: {}", connections)),
            ];
            let graph = Paragraph::new(span)
                .block(Block::default().borders(Borders::ALL))
                .style(Style::default());
            f.render_widget(graph, slice[1]);
        }
        2 => {
            let id = &app.node_info.infos[2].id;
            let connections = app.node_info.infos[2].connections;
            let span = vec![
                Spans::from(format!("NodeId: {}", id)),
                Spans::from(format!("Number of connections: {}", connections)),
            ];
            let graph = Paragraph::new(span)
                .block(Block::default().borders(Borders::ALL))
                .style(Style::default());
            f.render_widget(graph, slice[1]);
        }
        3 => {
            let id = &app.node_info.infos[3].id;
            let connections = app.node_info.infos[3].connections;
            let span = vec![
                Spans::from(format!("NodeId: {}", id)),
                Spans::from(format!("Number of connections: {}", connections)),
            ];
            let graph = Paragraph::new(span)
                .block(Block::default().borders(Borders::ALL))
                .style(Style::default());
            f.render_widget(graph, slice[1]);
        }
        4 => {
            let id = &app.node_info.infos[4].id;
            let connections = app.node_info.infos[3].connections;
            let span = vec![
                Spans::from(format!("NodeId: {}", id)),
                Spans::from(format!("Number of connections: {}", connections)),
            ];
            let graph = Paragraph::new(span)
                .block(Block::default().borders(Borders::ALL))
                .style(Style::default());
            f.render_widget(graph, slice[1]);
        }
        _ => {
            // do something
        }
    }
}

// render_info(index)
// return info at index
// render info
