use crate::{
    app::App,
    //node_info::{NodeInfo, NodeInfoView},
};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Spans,
    widgets::{Block, Borders, List, ListItem},
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

    //// call make info here
    //let text: Vec<Spans> = app.node_info.info.iter().map(|i| Spans::from(i.to_string())).collect();

    //let graph = Paragraph::new(text)
    //    .block(Block::default().borders(Borders::ALL))
    //    .style(Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD));

    //f.render_widget(graph, slice[1]);
}
