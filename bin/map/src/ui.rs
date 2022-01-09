use crate::app::App;
use tui::{
    backend::Backend,
    style::{Color, Modifier, Style},
    text::{Spans},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

pub fn ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let size = f.size();

    let items: Vec<ListItem> = app
        .nodes
        .items
        .iter()
        .map(|i| {
            let lines = vec![Spans::from(i.to_string())];
            ListItem::new(lines).style(Style::default())
        })
        .collect();

    // Create a List from all list items and highlight the currently selected one
    let items = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("List of nodes"))
        .highlight_style(Style::default().bg(Color::Black).add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");

    // Render the item list
    f.render_stateful_widget(items, size, &mut app.nodes.state);
}
