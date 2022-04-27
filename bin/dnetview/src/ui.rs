////use crate::model::SelectableObject;
//use crate::view::View;
//use log::debug;
//
//use tui::{
//    backend::Backend,
//    layout::{Constraint, Direction, Layout, Rect},
//    style::Style,
//    text::{Span, Spans},
//    widgets::{Block, Borders, List, ListItem, Paragraph},
//    Frame,
//};
//
//pub fn ui<B: Backend>(f: &mut Frame<'_, B>, mut view: View) {
//    let list_margin = 2;
//    let list_direction = Direction::Horizontal;
//    let list_cnstrnts = vec![Constraint::Percentage(50), Constraint::Percentage(50)];
//
//    let mut nodes = Vec::new();
//    let style = Style::default();
//
//    // TODO: this is insanely nested. pass SelectableObjects to different views
//    for id in &view.id_list.ids {
//        let mut lines: Vec<Spans> = Vec::new();
//        //debug!("{}", id);
//        // this only needs to be node ids
//        match &view.info_list.infos.get(id) {
//            Some(node) => {
//                match node {
//                    SelectableObject::Node(node_info) => {
//                        let name_span = Span::raw(node_info.node_name.to_string());
//                        lines.push(Spans::from(name_span));
//                        //let mut lines = vec![Spans::from(name_span)];
//                        //let id = &node_info.node_id;
//                        //let name = &node_info.node_name;
//                        //for child in &node_info.children {
//                        //    match child.session_name.as_str() {
//                        //        "Outgoing" => {
//                        //            lines.push(Spans::from(Span::styled(
//                        //                "   Outgoing",
//                        //                Style::default(),
//                        //            )));
//                        //        }
//                        //        "Incoming" => {
//                        //            lines.push(Spans::from(Span::styled(
//                        //                "   Outgoing",
//                        //                Style::default(),
//                        //            )));
//                        //        }
//                        //        "Manual" => {
//                        //            lines.push(Spans::from(Span::styled(
//                        //                "   Outgoing",
//                        //                Style::default(),
//                        //            )));
//                        //        }
//                        //        _ => {}
//                        //    }
//                        //    for child in &child.children {
//                        //        // do something
//                        //    }
//                        //}
//                        //if !node.outbound.iter().all(|node| node.is_empty) {
//                        //    lines.push(Spans::from(Span::styled("   Outgoing", Style::default())));
//                        //}
//
//                        // ok
//                    }
//                    _ => {
//                        // ok
//                    }
//                }
//                //for outbound in &node.outbound.clone() {      LOG_TARGETS=net cargo run -- -vv --slots 5 --seed 0.0.0.0:9999 --irc 127.0.0.1:6668 --rpc 127.0.0.1:8000
//                //    for slot in outbound.slots.clone() {
//                //        let addr = Span::styled(format!("       {}", slot.addr), style);
//                //        let msg: Span = match slot.channel.last_status.as_str() {
//                //            "recv" => Span::styled(
//                //                format!("               [R: {}]", slot.channel.last_msg),
//                //                style,
//                //            ),
//                //            "sent" => Span::styled(
//                //                format!("               [S: {}]", slot.channel.last_msg),
//                //                style,
//                //            ),
//                //            a => Span::styled(a.to_string(), style),
//                //        };
//                //        lines.push(Spans::from(vec![addr, msg]));
//                //    }
//                //}
//                //if !node.inbound.iter().all(|node| node.is_empty) {
//                //    lines.push(Spans::from(Span::styled("   Incoming", Style::default())));
//                //}
//                //for inbound in &node.inbound {
//                //    let addr = Span::styled(format!("       {}", inbound.connected), style);
//                //    let msg: Span = match inbound.channel.last_status.as_str() {
//                //        "recv" => Span::styled(
//                //            format!("               [R: {}]", inbound.channel.last_msg),
//                //            style,
//                //        ),
//                //        "sent" => Span::styled(
//                //            format!("               [R: {}]", inbound.channel.last_msg),
//                //            style,
//                //        ),
//                //        a => Span::styled(a.to_string(), style),
//                //    };
//                //    lines.push(Spans::from(vec![addr, msg]));
//                //}
//                //lines.push(Spans::from(Span::styled("   Manual", Style::default())));
//                //for connect in &node.manual {
//                //    lines.push(Spans::from(Span::styled(format!("       {}", connect.key), style)));
//                //}
//            }
//            None => {
//                // TODO
//                debug!("This is also a bug");
//            }
//        }
//
//        // need list of all ids here
//        let ids = ListItem::new(lines);
//        nodes.push(ids);
//    }
//
//    let nodes =
//        List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");
//    let slice = Layout::default()
//        .direction(list_direction)
//        .margin(list_margin)
//        .constraints(list_cnstrnts)
//        .split(f.size());
//
//    f.render_stateful_widget(nodes, slice[0], &mut view.id_list.state);
//
//    render_info_right(view.clone(), f, slice);
//}
//
//fn render_info_right<B: Backend>(_view: View, f: &mut Frame<'_, B>, slice: Vec<Rect>) {
//    let span = vec![];
//    let graph =
//        Paragraph::new(span).block(Block::default().borders(Borders::ALL)).style(Style::default());
//    f.render_widget(graph, slice[1]);
//}
