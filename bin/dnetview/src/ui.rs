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

// create top level outgoing widget
pub fn make_oframe() -> ConnectBox {
    let ot_len = 4;
    let ot_width = 8;
    let ot_align = Alignment::Left;
    let ot_cnstrnt = vec![Constraint::Percentage(100)];
    let ot_widget = InfoBox::new(ot_len, ot_width, ot_align, ot_cnstrnt);

    let om_len = 5;
    let om_width = 8;
    let om_align = Alignment::Right;
    let om_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];
    let om_widget = InfoBox::new(om_len, om_width, om_align, om_cnstrnt);

    let os_len = 5;
    let os_width = 10;
    let os_align = Alignment::Left;
    let os_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];
    let os_widget = InfoBox::new(os_len, os_width, os_align, os_cnstrnt);

    let oframe = ConnectBox::new(ot_widget, os_widget, om_widget);
    oframe
}

// create top level ingoing widget
pub fn make_iframe(oframe: ConnectBox) -> ConnectBox {
    let it_len = oframe.addrs_box.len;
    let it_width = 8;
    let it_align = Alignment::Left;
    let it_cnstrnt = vec![Constraint::Percentage(100)];
    let it_widget = InfoBox::new(it_len, it_width, it_align, it_cnstrnt);

    let is_len = oframe.addrs_box.len + oframe.title_box.len + 1;
    let is_width = 10;
    let is_align = Alignment::Left;
    let is_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];
    let is_widget = InfoBox::new(is_len, is_width, is_align, is_cnstrnt);

    let im_len = is_len;
    let im_width = 8;
    let im_align = Alignment::Right;
    let im_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];
    let im_widget = InfoBox::new(im_len, im_width, im_align, im_cnstrnt);

    let iframe = ConnectBox::new(it_widget, is_widget, im_widget);
    iframe
}

// create top level manual widget
pub fn make_mframe(iframe: ConnectBox) -> ConnectBox {
    let mt_len = iframe.title_box.len + iframe.addrs_box.len + 1;
    let mt_width = 8;
    let mt_align = Alignment::Left;
    let mt_cnstrnt = vec![Constraint::Percentage(100)];
    let mt_widget = InfoBox::new(mt_len, mt_width, mt_align, mt_cnstrnt);

    let mk_len = mt_len + 1;
    let mk_width = 10;
    let mk_align = Alignment::Left;
    let mk_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];
    let mk_widget = InfoBox::new(mk_len, mk_width, mk_align, mk_cnstrnt);

    let mm_len = mt_len + 1;
    let mm_width = 10;
    let mm_align = Alignment::Left;
    let mm_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];
    let mm_widget = InfoBox::new(mm_len, mm_width, mm_align, mm_cnstrnt);

    let mframe = ConnectBox::new(mt_widget.clone(), mk_widget.clone(), mm_widget);
    mframe
}

pub fn ui<B: Backend>(f: &mut Frame<'_, B>, mut view: View) {
    // for id in node_id {
    //     let Outbox = get_outbound(node_id)
    //     make_frame(spans.len())
    //     draw(spans)
    // }
    // render_outbound(node_id);
    // render_inbound(node_id)
    // render_manual(node_id)
    let oframe = make_oframe();
    get_and_draw_outbound(f, view.clone(), oframe.clone());
    let iframe = make_iframe(oframe.clone());
    get_and_draw_inbound(f, view.clone(), iframe.clone(), oframe.clone());
    let mframe = make_mframe(iframe.clone());
    draw_manual(f, view.clone(), mframe.clone());

    let top_widget = NodeBox::new(oframe, iframe, mframe);

    let list_margin = 2;
    let list_direction = Direction::Horizontal;
    let list_cnstrnts = vec![Constraint::Percentage(50), Constraint::Percentage(50)];

    let prev_len = top_widget.manual.addrs_box.len;
    //draw_list(list_margin, prev_len, list_direction, list_cnstrnts, view.clone(), top_widget, f);
    let mut nodes = Vec::new();

    for id in &view.id_list.node_id {
        let mut lines = vec![Spans::from(id.to_string())];
        for _i in 1..prev_len {
            lines.push(Spans::from(""));
        }
        let ids = ListItem::new(lines).style(Style::default());
        nodes.push(ids)
    }

    let nodes =
        List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");
    // this is just the box around the list. not the actual list
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

#[derive(Clone)]
pub struct NodeBox {
    pub outbound: ConnectBox,
    pub inbound: ConnectBox,
    pub manual: ConnectBox,
}

impl NodeBox {
    pub fn new(outbound: ConnectBox, inbound: ConnectBox, manual: ConnectBox) -> NodeBox {
        NodeBox { outbound, inbound, manual }
    }
}

#[derive(Clone)]
pub struct ConnectBox {
    pub title_box: InfoBox,
    pub addrs_box: InfoBox,
    pub msgs_box: InfoBox,
}

impl ConnectBox {
    pub fn new(title_box: InfoBox, addrs_box: InfoBox, msgs_box: InfoBox) -> ConnectBox {
        ConnectBox { title_box, addrs_box, msgs_box }
    }
    pub fn get_total_len(self) -> usize {
        let t_len = self.title_box.len;
        let a_len = self.addrs_box.len;
        let m_len = self.msgs_box.len;
        let total_len = t_len + a_len + m_len;
        total_len
    }
}

// Lowest level widgets
#[derive(Clone)]
pub struct InfoBox {
    pub len: usize,
    pub width: usize,
    pub align: Alignment,
    pub cnstrnts: Vec<Constraint>,
}

impl InfoBox {
    pub fn new(len: usize, width: usize, align: Alignment, cnstrnts: Vec<Constraint>) -> InfoBox {
        InfoBox { len, width, align, cnstrnts }
    }

    pub fn update(mut self, len: usize) {
        self.len = len
    }

    pub fn draw<B: Backend>(self, vec: Vec<Spans>, f: &mut Frame<'_, B>) {
        let slice = Layout::default()
            .direction(Direction::Horizontal)
            .horizontal_margin(self.width as u16)
            .vertical_margin(self.len as u16)
            .constraints(self.cnstrnts.as_ref())
            .split(f.size());
        let graph = Paragraph::new(vec).style(Style::default()).alignment(self.align);
        f.render_widget(graph, slice[0]);
    }
    pub fn get_len(self) -> usize {
        return self.len;
    }
}

// loop through all connected nodes in Model
// parse outbound data by creating a text object called Vec<Spans>
// send Vec<Spans> to render_widget()
fn get_and_draw_outbound<B: Backend>(f: &mut Frame<'_, B>, view: View, oframe: ConnectBox) {
    for id in &view.id_list.node_id {
        let mut titles = Vec::new();
        let mut msgs = Vec::new();
        let mut slots = Vec::new();
        let mut data = Vec::new();

        //debug!("Looping through nodes: {}", id);
        match &view.info_list.infos.get(id) {
            Some(node) => {
                data.push(id.to_string());
                for outbound in &node.outbound.clone() {
                    titles.push(Spans::from(Span::styled("Outgoing", Style::default())));
                    data.push("Outgoing".to_string());
                    for slot in outbound.slots.clone() {
                        if slot.addr.as_str() == "Empty" {
                            slots.push(Spans::from(format!("{}", slot.addr.as_str())));
                            data.push("Empty".to_string());
                        } else {
                            slots.push(Spans::from(format!("{}", slot.addr)));
                            data.push(format!("{}", slot.addr));
                        }
                        match slot.channel.last_status.as_str() {
                            "recv" => {
                                msgs.push(Spans::from(format!("[R: {}]", slot.channel.last_msg)));
                                data.push(format!("{}", slot.channel.last_msg));
                            }
                            "sent" => {
                                msgs.push(Spans::from(format!("[S: {}]", slot.channel.last_msg)));
                                data.push(format!("{}", slot.channel.last_msg));
                            }
                            "Null" => {
                                data.push("Null".to_string());
                            }
                            _ => {
                                // TODO
                                debug!("This is a bug");
                            }
                        }
                    }
                }
            }
            None => {
                debug!("NONE VALUE TRIGGERD");
                // TODO: Error
            }
        }
        debug!("{:?}", data);
        oframe.title_box.clone().draw(titles.clone(), f);
        let t_len2 = titles.clone().len();
        oframe.title_box.clone().update(t_len2);

        //oframe.addrs.clone().draw(slots.clone(), f);
        //let s_len2 = slots.clone().len();
        //oframe.addrs.clone().update(s_len2);

        //oframe.msgs.clone().draw(msgs.clone(), f);
        //let m_len2 = msgs.clone().len();
        //oframe.msgs.clone().update(m_len2);
    }
}

fn print_type_of<T>(_: &T) {
    debug!("{}", std::any::type_name::<T>())
}
// loop through all connected nodes in Model
// parse inbound data by creating a text object called Vec<Spans>
// send Vec<Spans> to render_widget()
fn get_and_draw_inbound<B: Backend>(
    f: &mut Frame<'_, B>,
    view: View,
    iframe: ConnectBox,
    outframe: ConnectBox,
) {
    let slots_len = outframe.addrs_box.get_len();

    for id in &view.id_list.node_id {
        // create a new data thing
        let mut titles = Vec::new();
        let mut addrs = Vec::new();
        let mut msgs = Vec::new();
        let mut data = Vec::new();
        // need to have access to slots_len
        for _i in 1..slots_len {
            titles.push(Spans::from(""));
        }
        //debug!("Looping through nodes: {}", id);
        match &view.info_list.infos.get(id) {
            Some(node) => {
                data.push(id.to_string());
                for connect in &node.inbound {
                    titles.push(Spans::from(Span::styled("Incoming", Style::default())));
                    data.push("Incoming".to_string());
                    addrs.push(Spans::from(connect.connected.clone()));
                    data.push(connect.connected.clone());

                    match connect.channel.last_status.as_str() {
                        "recv" => {
                            msgs.push(Spans::from(format!("[R: {}]", connect.channel.last_msg)));
                            data.push(format!("[R: {}]", connect.channel.last_msg));
                        }
                        "sent" => {
                            msgs.push(Spans::from(format!("[S: {}]", connect.channel.last_msg)));
                            data.push(format!("[S: {}]", connect.channel.last_msg));
                        }
                        "Null" => {
                            data.push("Null".to_string());
                        }
                        _ => {
                            // TODO
                            debug!("This is a bug");
                        }
                    }
                }
            }
            None => {
                debug!("NONE VALUE TRIGGERD");
                // This should never happen. TODO: make this an error.
            }
        }
        debug!("{:?}", data);
        //iframe.title.clone().draw(titles.clone(), f);
        //let t_len2 = titles.clone().len();
        //iframe.title.clone().update(t_len2);
        //iframe.addrs.clone().draw(addrs.clone(), f);
        //let s_len2 = addrs.clone().len();
        //iframe.addrs.clone().update(s_len2);
        //iframe.msgs.clone().draw(msgs.clone(), f);
        //let m_len2 = msgs.clone().len();
        //iframe.msgs.clone().update(m_len2);
    }
}

fn draw_manual<B: Backend>(f: &mut Frame<'_, B>, view: View, mframe: ConnectBox) {
    let mut titles = Vec::new();
    let mut keys = Vec::new();

    titles.push(Spans::from(Span::styled("Manual", Style::default())));
    for id in &view.id_list.node_id {
        match &view.info_list.infos.get(id) {
            Some(connects) => {
                for _conn in &connects.manual {
                    keys.push(Spans::from(format!("{}", connects.manual[0].key)));
                }
            }
            None => {
                // TODO
            }
        }
    }

    //mframe.title.clone().draw(titles.clone(), f);
    //let t_len2 = titles.clone().len();
    //mframe.title.clone().update(t_len2);

    //mframe.addrs.clone().draw(keys.clone(), f);
    //let s_len2 = keys.clone().len();
    //mframe.addrs.clone().update(s_len2);
}

//fn draw_list<B: Backend>(
//    margin: u16,
//    prev_len: usize,
//    direction: Direction,
//    cnstrnts: Vec<Constraint>,
//    mut view: View,
//    connects: NodeBox,
//    f: &mut Frame<'_, B>,
//) {
//    let mut nodes = Vec::new();
//
//    for id in &view.id_list.node_id {
//        let mut lines = vec![Spans::from(id.to_string())];
//        // need total_len
//        // process_outbound
//        // process_inbound
//        // process_manual
//        for _i in 1..prev_len {
//            lines.push(Spans::from(""));
//        }
//        let ids = ListItem::new(lines).style(Style::default());
//        nodes.push(ids)
//    }
//
//    let nodes =
//        List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");
//    // this is just the box around the list. not the actual list
//    let slice =
//        Layout::default().direction(direction).margin(margin).constraints(cnstrnts).split(f.size());
//
//    f.render_stateful_widget(nodes, slice[0], &mut view.id_list.state);
//}
