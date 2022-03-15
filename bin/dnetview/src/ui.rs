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

    for id in &view.id_list.node_id {
        let mut msgs = Vec::new();
        let mut lines = vec![Spans::from(id.to_string())];
        match &view.info_list.infos.get(id) {
            Some(node) => {
                //data.push(id.to_string());
                for outbound in &node.outbound.clone() {
                    lines.push(Spans::from(Span::styled("Outgoing", Style::default())));
                    //data.push("Outgoing".to_string());
                    for slot in outbound.slots.clone() {
                        if slot.addr.as_str() == "Empty" {
                            msgs.push(Spans::from(format!("{}", slot.addr.as_str())));
                            //data.push("Empty".to_string());
                        } else {
                            msgs.push(Spans::from(format!("{}", slot.addr)));
                            //data.push(format!("{}", slot.addr));
                        }
                        match slot.channel.last_status.as_str() {
                            "recv" => {
                                msgs.push(Spans::from(format!("[R: {}]", slot.channel.last_msg)));
                                //data.push(format!("{}", slot.channel.last_msg));
                            }
                            "sent" => {
                                msgs.push(Spans::from(format!("[S: {}]", slot.channel.last_msg)));
                                //data.push(format!("{}", slot.channel.last_msg));
                            }
                            "Null" => {
                                //data.push("Null".to_string());
                            }
                            _ => {
                                // TODO
                                debug!("This is a bug");
                            }
                        }
                    }
                }
                for connect in &node.inbound {
                    lines.push(Spans::from(Span::styled("Incoming", Style::default())));
                    //lines.push("Incoming".to_string());
                    lines.push(Spans::from(connect.connected.clone()));
                    //data.push(connect.connected.clone());

                    match connect.channel.last_status.as_str() {
                        "recv" => {
                            msgs.push(Spans::from(format!("[R: {}]", connect.channel.last_msg)));
                            //data.push(format!("[R: {}]", connect.channel.last_msg));
                        }
                        "sent" => {
                            msgs.push(Spans::from(format!("[S: {}]", connect.channel.last_msg)));
                            //data.push(format!("[S: {}]", connect.channel.last_msg));
                        }
                        "Null" => {
                            //data.push("Null".to_string());
                        }
                        _ => {
                            // TODO
                            debug!("This is a bug");
                        }
                    }
                }
                for connect in &node.manual {
                    lines.push(Spans::from(Span::styled("Manual", Style::default())));
                    lines.push(Spans::from(format!("{}", connect.key)));
                }
            }
            None => {
                debug!("NONE VALUE TRIGGERD");
                // TODO: Error
            }
        }
        let ids = ListItem::new(lines);
        nodes.push(ids)
    }

    let nodes =
        List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");
    let slice = Layout::default()
        .direction(list_direction)
        .margin(list_margin)
        .constraints(list_cnstrnts)
        .split(f.size());

    f.render_stateful_widget(nodes, slice[0], &mut view.id_list.state);
    //render_info_right(view.clone(), f, slice);
}

//fn render_info_right<B: Backend>(_view: View, f: &mut Frame<'_, B>, slice: Vec<Rect>) {
//    let span = vec![];
//    let graph =
//        Paragraph::new(span).block(Block::default().borders(Borders::ALL)).style(Style::default());
//    f.render_widget(graph, slice[1]);
//}
