use crate::view::View;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Spans,
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
            let line1 = Spans::from(id.to_string());
            ListItem::new(vec![line1]).style(Style::default())
        })
        .collect();

    let nodes = List::new(nodes)
        .block(Block::default().borders(Borders::ALL))
        .highlight_style(Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD));

    f.render_stateful_widget(nodes, slice[0], &mut view.id_list.state);

    let index = view.info_list.index;

    render_info_left(view.clone(), f, index);
    render_info_right(view.clone(), f, index, slice);
}

fn render_info_left<B: Backend>(view: View, f: &mut Frame<'_, B>, index: usize) {
    let slice = Layout::default()
        .direction(Direction::Horizontal)
        .vertical_margin(4)
        .horizontal_margin(7)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(f.size());

    let info = &view.info_list.infos;

    let iconnects = info[index].incoming.clone();
    let oconnects = info[index].outgoing.clone();

    let mut iconnect_ids = Vec::new();
    let mut oconnect_ids = Vec::new();

    if !iconnects.is_empty() {
        for connect in iconnects {
            iconnect_ids.push(connect.id);
        }
    }
    if !oconnects.is_empty() {
        for connect in oconnects {
            oconnect_ids.push(connect.id);
        }
    }
    let span = vec![
        Spans::from(format!("Outgoing connections:")),
        Spans::from(format!("   {}", iconnect_ids[0])),
        Spans::from(format!("   {}", iconnect_ids[1])),
        Spans::from(format!("")),
        Spans::from(format!("Incoming connections:")),
        Spans::from(format!("   {}", oconnect_ids[0])),
        Spans::from(format!("   {}", oconnect_ids[1])),
    ];
    let graph = Paragraph::new(span).block(Block::default().style(Style::default()));
    f.render_widget(graph, slice[0]);
}

fn render_info_right<B: Backend>(view: View, f: &mut Frame<'_, B>, index: usize, slice: Vec<Rect>) {
    let info = &view.info_list.infos;

    let iconnects = info[index].incoming.clone();
    let oconnects = info[index].outgoing.clone();

    let mut iconnect_msgs = Vec::new();
    let mut oconnect_msgs = Vec::new();

    if !iconnects.is_empty() {
        for connect in iconnects {
            iconnect_msgs.push(connect.message)
        }
    }
    if !oconnects.is_empty() {
        for connect in oconnects {
            oconnect_msgs.push(connect.message);
        }
    }
    let span = vec![
        Spans::from(format!("Last message:")),
        Spans::from(format!("")),
        Spans::from(format!("{}", oconnect_msgs[0])),
        Spans::from(format!("{}", oconnect_msgs[1])),
        Spans::from(format!("")),
        Spans::from(format!("")),
        Spans::from(format!("{}", oconnect_msgs[0])),
        Spans::from(format!("{}", oconnect_msgs[1])),
    ];
    let graph =
        Paragraph::new(span).block(Block::default().borders(Borders::ALL)).style(Style::default());
    f.render_widget(graph, slice[1]);
}
