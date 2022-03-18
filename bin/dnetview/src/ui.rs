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
    // we write all the span data to a Vec<String> for debugging purposes
    let mut data = Vec::new();
    let style = Style::default();

    for id in &view.id_list.node_id {
        let id_span = Span::raw(id.to_string());
        let mut lines = vec![Spans::from(id_span)];
        data.push(id.to_string());

        match &view.info_list.infos.get(id) {
            // TODO:
            //      1. Only print 'outbound' or 'inbound':
            //              * if in/outbound is not empty
            //              * only print it once
            //
            //      2. Fix error whereby duplicates outbounds are being printed (nested loop)
            Some(node) => {
                // create the title
                if !node.outbound.is_empty() {
                    lines.push(Spans::from(Span::styled("   Outgoing", Style::default())));
                    data.push("Outgoing".to_string());
                }
                for outbound in &node.outbound.clone() {
                    debug!("{:?}", outbound);
                    if outbound.is_empty == false {
                        for slot in outbound.slots.clone() {
                            let addr = Span::styled(format!("       {}", slot.addr), style);
                            data.push(format!("{}", slot.addr));
                            let msg: Span = match slot.channel.last_status.as_str() {
                                "recv" => Span::styled(
                                    format!("               [R: {}]", slot.channel.last_msg),
                                    style,
                                ),
                                "sent" => Span::styled(
                                    format!("               [S: {}]", slot.channel.last_msg),
                                    style,
                                ),
                                a => Span::styled(a.to_string(), style),
                            };
                            data.push(format!("{}", slot.channel.last_msg));
                            lines.push(Spans::from(vec![addr, msg]));
                        }
                    }
                }
                // create the title
                if !node.inbound.is_empty() {
                    lines.push(Spans::from(Span::styled("   Incoming", Style::default())));
                    data.push("Incoming".to_string());
                }
                for inbound in &node.inbound {
                    if inbound.is_empty == false {
                        let addr = Span::styled(format!("       {}", inbound.connected), style);
                        data.push(format!("{}", inbound.connected));
                        let msg: Span = match inbound.channel.last_status.as_str() {
                            "recv" => Span::styled(
                                format!("               [R: {}]", inbound.channel.last_msg),
                                style,
                            ),
                            "sent" => Span::styled(
                                format!("               [R: {}]", inbound.channel.last_msg),
                                style,
                            ),
                            a => Span::styled(a.to_string(), style),
                        };
                        data.push(format!("{}", inbound.channel.last_msg));
                        lines.push(Spans::from(vec![addr, msg]));
                    }
                }
                // create the title
                if !node.manual.is_empty() {
                    lines.push(Spans::from(Span::styled("   Manual", Style::default())));
                    data.push("Manual".to_string());
                }
                for connect in &node.manual {
                    lines.push(Spans::from(Span::styled(format!("       {}", connect.key), style)));
                    data.push(format!("{}", connect.key));
                }
            }
            None => {
                // TODO
                debug!("This is also a bug");
            }
        }

        debug!("{:?}", data);
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
