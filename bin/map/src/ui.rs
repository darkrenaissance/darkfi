use crate::view::View;
use log::debug;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
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

    let info = &view.info_list.infos;
    //debug!("UI ID LIST: {:?}", view.id_list.node_id);
    //debug!("UI INFO LIST: {:?}", view.info_list.infos);
    let index = view.info_list.index;

    let iconnects = info[index].incoming.clone();
    let oconnects = info[index].outgoing.clone();

    let nodes: Vec<ListItem> = view
        .id_list
        .node_id
        .iter()
        .map(|id| {
            let mut lines = vec![Spans::from(id.to_string())];
            for line in lines.clone() {
                lines.push(Spans::from(Span::styled("  Outgoing:", Style::default())));
                lines.push(Spans::from(format!(
                    "    {}         [R: {}]",
                    oconnects[0].id, oconnects[0].message
                )));
                lines.push(Spans::from(format!(
                    "    {}         [S: {}]",
                    oconnects[1].id, oconnects[1].message
                )));
                lines.push(Spans::from(Span::styled("  Incoming:", Style::default())));
                lines.push(Spans::from(format!(
                    "    {}         [R: {}]",
                    iconnects[0].id, iconnects[0].message
                )));
                lines.push(Spans::from(format!(
                    "    {}         [S: {}]",
                    iconnects[1].id, iconnects[1].message
                )));
            }
            ListItem::new(lines).style(Style::default())
        })
        .collect();

    let nodes =
        List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");

    f.render_stateful_widget(nodes, slice[0], &mut view.id_list.state);

    let index = view.info_list.index;

    render_info_right(view.clone(), f, index, slice);
}

fn render_info_right<B: Backend>(view: View, f: &mut Frame<'_, B>, index: usize, slice: Vec<Rect>) {
    let span = vec![];
    let graph =
        Paragraph::new(span).block(Block::default().borders(Borders::ALL)).style(Style::default());
    f.render_widget(graph, slice[1]);
}
