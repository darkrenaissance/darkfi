use crate::view::View;
use log::debug;

use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

pub fn ui<B: Backend>(f: &mut Frame<'_, B>, mut view: View) {
    let list_margin = 2;
    let list_direction = Direction::Horizontal;
    let list_cnstrnts = vec![Constraint::Percentage(50), Constraint::Percentage(50)];

    let mut nodes = Vec::new();
    let style = Style::default();

    // lines.push(sublist)
    // either have one hashmap w value as enum or have type info in hashset
    for id in &view.id_list.node_id {
        let id_span = Span::raw(id.to_string());
        let mut lines = vec![Spans::from(id_span)];

        // create a new vector of addresses
        // render as a sub node
        match &view.info_list.infos.get(id) {
            Some(node) => {
                //if !node.outbound.iter().all(|node| node.is_empty) {
                //    lines.push(Spans::from(Span::styled("   Outgoing", Style::default())));
                //}
                //for outbound in &node.outbound.clone() {
                //    for slot in outbound.slots.clone() {
                //        let addr = Span::styled(format!("       {}", slot.addr), style);
                //        let msg: Span = match slot.channel.last_status.as_str() {
                //            "recv" => Span::styled(
                //                format!("               [R: {}]", slot.channel.last_msg),
                //                style,
                //            ),
                //            "sent" => Span::styled(
                //                format!("               [S: {}]", slot.channel.last_msg),
                //                style,
                //            ),
                //            a => Span::styled(a.to_string(), style),
                //        };
                //        lines.push(Spans::from(vec![addr, msg]));
                //    }
                //}
                //if !node.inbound.iter().all(|node| node.is_empty) {
                //    lines.push(Spans::from(Span::styled("   Incoming", Style::default())));
                //}
                //for inbound in &node.inbound {
                //    let addr = Span::styled(format!("       {}", inbound.connected), style);
                //    let msg: Span = match inbound.channel.last_status.as_str() {
                //        "recv" => Span::styled(
                //            format!("               [R: {}]", inbound.channel.last_msg),
                //            style,
                //        ),
                //        "sent" => Span::styled(
                //            format!("               [R: {}]", inbound.channel.last_msg),
                //            style,
                //        ),
                //        a => Span::styled(a.to_string(), style),
                //    };
                //    lines.push(Spans::from(vec![addr, msg]));
                //}
                //lines.push(Spans::from(Span::styled("   Manual", Style::default())));
                //for connect in &node.manual {
                //    lines.push(Spans::from(Span::styled(format!("       {}", connect.key), style)));
                //}
            }
            None => {
                // TODO
                debug!("This is also a bug");
            }
        }

        let ids = ListItem::new(lines);
        nodes.push(ids);
    }

    let nodes =
        List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");
    let slice = Layout::default()
        .direction(list_direction)
        .margin(list_margin)
        .constraints(list_cnstrnts)
        .split(f.size());

    f.render_stateful_widget(nodes, slice[0], &mut view.id_list.state);

    render_info_right(view.clone(), f, slice);
}

fn render_info_right<B: Backend>(_view: View, f: &mut Frame<'_, B>, slice: Vec<Rect>) {
    let span = vec![];
    let graph =
        Paragraph::new(span).block(Block::default().borders(Borders::ALL)).style(Style::default());
    f.render_widget(graph, slice[1]);
}
