use crate::view::View;

use tui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
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

    let nodes: Vec<ListItem> = view
        .id_list
        .node_id
        .iter()
        .map(|id| {
            let lines = vec![Spans::from(id.to_string())];
            ListItem::new(lines).style(Style::default())
        })
        .collect();

    let nodes =
        List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");

    f.render_stateful_widget(nodes, slice[0], &mut view.id_list.state);

    let index = view.info_list.index;

    render_info_left(view.clone(), f);
    render_info_right(view.clone(), f, index, slice);
}

fn render_info_left<B: Backend>(view: View, f: &mut Frame<'_, B>) {
    let len = draw_outbound(view.clone(), f);
    let len = draw_inbound(view.clone(), f, len);
    draw_manual(view.clone(), f, len);
}

// We're not doing anything here right now.
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

fn draw_outbound<B: Backend>(view: View, f: &mut Frame<'_, B>) -> usize {
    let t_len = 4;
    let m_len = 4;
    let s_len = 4;

    let t_width = 8;
    let m_width = 8;
    let s_width = 10;

    let t_align = Alignment::Left;
    let m_align = Alignment::Right;
    let s_align = Alignment::Left;

    let t_cnstrnt = vec![Constraint::Percentage(100)];
    let m_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];
    let s_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];

    let mut titles = Vec::new();
    let mut msgs = Vec::new();
    let mut slots = Vec::new();

    for id in &view.id_list.node_id {
        match &view.info_list.infos.get(id) {
            Some(connects) => {
                titles.push(Spans::from(Span::styled("Outbound:", Style::default())));
                for slot in &connects.outbound[0].slots {
                    slots.push(Spans::from(format!("{}", slot.addr)));
                    match slot.channel.last_status.as_str() {
                        "recv" => {
                            msgs.push(Spans::from(format!("[R: {}]", slot.channel.last_msg)));
                        }
                        "sent" => {
                            msgs.push(Spans::from(format!("[S: {}]", slot.channel.last_msg)));
                        }
                        _ => {
                            // TODO: right now we do nothing with these values
                        }
                    }
                }
            }
            None => {
                // TODO: Error
            }
        }
    }

    let s_len = s_len + titles.len() as u16;
    let m_len = m_len + titles.len() as u16;

    draw(t_len, t_width, titles, t_align, t_cnstrnt, f);
    draw(s_len, s_width, slots, s_align, s_cnstrnt, f);
    draw(m_len, m_width, msgs, m_align, m_cnstrnt, f);

    let total_len = t_len as usize + s_len as usize;
    total_len
}

fn draw_inbound<B: Backend>(view: View, f: &mut Frame<'_, B>, len: usize) -> usize {
    let t_len = len as u16;
    let a_len = len as u16;
    let m_len = len as u16;

    let t_width = 8;
    let a_width = 10;
    let m_width = 10;

    let t_align = Alignment::Left;
    let m_align = Alignment::Right;
    let a_align = Alignment::Left;

    let t_cnstrnt = vec![Constraint::Percentage(100)];
    let a_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];
    let m_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];

    let mut titles = Vec::new();
    let mut addrs = Vec::new();
    let mut msgs = Vec::new();

    for id in &view.id_list.node_id {
        match &view.info_list.infos.get(id) {
            Some(connection) => {
                if !connection.inbound.is_empty() {
                    for connect in &connection.inbound {
                        addrs.push(Spans::from(""));
                        addrs.push(Spans::from(connect.connected.clone()));
                        match connect.channel.last_status.as_str() {
                            "recv" => {
                                msgs.push(Spans::from(""));
                                msgs.push(Spans::from(format!(
                                    "[R: {}]",
                                    connect.channel.last_msg
                                )));
                            }
                            "sent" => {
                                msgs.push(Spans::from(""));
                                msgs.push(Spans::from(format!(
                                    "[S: {}]",
                                    connect.channel.last_msg
                                )));
                            }
                            _ => {
                                // TODO: handle these values
                            }
                        }
                    }
                } else {
                    // Inbound connection is empty. Render empty data
                    titles.push(Spans::from(Span::styled("Inbound:", Style::default())));
                    addrs.push(Spans::from("Null"));
                    msgs.push(Spans::from("[R: Null]"));
                    msgs.push(Spans::from("[S: Null]"));
                }
            }
            None => {
                // This should never happen. TODO: make this an error.
            }
        }
    }

    let a_len = a_len + titles.len() as u16;
    let m_len = m_len + titles.len() as u16;

    draw(t_len, t_width, titles, t_align, t_cnstrnt, f);
    draw(a_len, a_width, addrs, a_align, a_cnstrnt, f);
    draw(m_len, m_width, msgs, m_align, m_cnstrnt, f);

    let total_len = t_len as usize + a_len as usize;
    total_len
}

fn draw_manual<B: Backend>(view: View, f: &mut Frame<'_, B>, len: usize) {
    let t_len = len as u16;
    let k_len = len as u16;

    let t_width = 8;
    let k_width = 10;

    let t_align = Alignment::Left;
    let k_align = Alignment::Left;

    let t_cnstrnt = vec![Constraint::Percentage(100)];
    let k_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];

    let mut titles = Vec::new();
    let mut keys = Vec::new();

    for id in &view.id_list.node_id {
        match &view.info_list.infos.get(id) {
            Some(connects) => {
                titles.push(Spans::from(Span::styled("Manual:", Style::default())));
                keys.push(Spans::from(""));
                keys.push(Spans::from(format!("Key: {}", connects.manual[0].key)));
                keys.push(Spans::from(""));
            }
            None => {
                // TODO
            }
        }
    }

    draw(t_len, t_width, titles, t_align, t_cnstrnt, f);
    draw(k_len, k_width, keys, k_align, k_cnstrnt, f);
}

fn draw<B: Backend>(
    length: u16,
    width: u16,
    vec: Vec<Spans>,
    align: Alignment,
    cnstrnts: Vec<Constraint>,
    f: &mut Frame<'_, B>,
) {
    let slice = Layout::default()
        .direction(Direction::Horizontal)
        .horizontal_margin(width)
        .vertical_margin(length)
        .constraints(cnstrnts.as_ref())
        .split(f.size());
    let graph = Paragraph::new(vec).style(Style::default()).alignment(align);
    f.render_widget(graph, slice[0]);
}
