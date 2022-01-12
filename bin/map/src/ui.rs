use crate::app::App;
use tui::{
    backend::Backend,
    style::{Color, Modifier, Style},
    text::Spans,
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

pub fn ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let size = f.size();

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

    // Create a List from all list nodes and highlight the currently selected one
    let nodes = List::new(nodes)
        .block(Block::default().borders(Borders::ALL).title("List of nodes"))
        .highlight_style(Style::default().bg(Color::Black).add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");

    // Render the item list
    f.render_stateful_widget(nodes, size, &mut app.node_list.state);
}
