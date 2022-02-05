use crate::app::App;
use async_std::sync::{Arc, Mutex};
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Spans,
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

pub async fn ui<B: Backend>(f: &mut Frame<'_, B>, app: Arc<Mutex<App>>) {
    let slice = Layout::default()
        .direction(Direction::Horizontal)
        .margin(2)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(f.size());

    let nodes: Vec<ListItem> = app
        .lock()
        .await
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

    f.render_stateful_widget(nodes, slice[0], &mut app.lock().await.id_list.state);

    let index = app.lock().await.info_list.index;

    render_info(app, f, index, slice).await;
}

async fn render_info<B: Backend>(
    app: Arc<Mutex<App>>,
    f: &mut Frame<'_, B>,
    index: usize,
    slice: Vec<Rect>,
) {
    let info = &app.lock().await.info_list.infos;
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
