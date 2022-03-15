use crate::view::View;
use log::debug;

use tui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout},
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

    for id in &view.id_list.node_id {
        let id_span = Span::raw(id.to_string());
        let mut lines = vec![Spans::from(id_span)];
        match &view.info_list.infos.get(id) {
            Some(node) => {
                for outbound in &node.outbound.clone() {
                    lines.push(Spans::from(Span::styled("   Outgoing", style)));
                    for slot in outbound.slots.clone() {
                        lines.push(Spans::from(Span::styled(
                            format!("       {}", slot.addr),
                            style,
                        )));
                        if slot.channel.last_status.as_str() != "Null" {
                            let msg: Span = match slot.channel.last_status.as_str() {
                                "recv" => {
                                    Span::styled(format!("[R: {}]", slot.channel.last_msg), style)
                                }
                                "sent" => {
                                    Span::styled(format!("[S: {}]", slot.channel.last_msg), style)
                                }
                                a => Span::styled(format!("{}", a), style),
                            };
                        } else {
                            // TODO
                        }
                    }
                }
                for connect in &node.inbound {
                    lines.push(Spans::from(Span::styled("   Incoming", Style::default())));
                    lines.push(Spans::from(Span::styled(
                        format!("       {}", connect.connected),
                        style,
                    )));

                    if connect.channel.last_status.as_str() != "Null" {
                        let msg: Span = match connect.channel.last_status.as_str() {
                            "recv" => {
                                Span::styled(format!("[R: {}]", connect.channel.last_msg), style)
                            }
                            "sent" => {
                                Span::styled(format!("[R: {}]", connect.channel.last_msg), style)
                            }
                            a => Span::styled(format!("{}", a), style),
                        };
                    };
                }
                for connect in &node.manual {
                    lines.push(Spans::from(Span::styled("   Manual", Style::default())));
                    lines.push(Spans::from(Span::styled(format!("       {}", connect.key), style)));
                }
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

    //let msgs = Paragraph::new(msgs).style(Style::default()).alignment(Alignment::Right);
    //f.render_widget(msgs, slice[0]);

    //render_info_right(view.clone(), f, slice);
}

//fn render_info_right<B: Backend>(_view: View, f: &mut Frame<'_, B>, slice: Vec<Rect>) {
//    let span = vec![];
//    let graph =
//        Paragraph::new(span).block(Block::default().borders(Borders::ALL)).style(Style::default());
//    f.render_widget(graph, slice[1]);
//}
