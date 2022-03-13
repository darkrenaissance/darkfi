use crate::view::View;
//use log::debug;

use tui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::Style,
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

pub fn make_oframe() -> NetFrame {
    let ot_len = 4;
    let ot_width = 8;
    let ot_align = Alignment::Left;
    let ot_cnstrnt = vec![Constraint::Percentage(100)];
    let ot_widget = NetWidget::new(ot_len, ot_width, ot_align, ot_cnstrnt);

    // create outbound msg widget
    let om_len = 5;
    let om_width = 8;
    let om_align = Alignment::Right;
    let om_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];
    let om_widget = NetWidget::new(om_len, om_width, om_align, om_cnstrnt);

    // create outbound slot widget
    let os_len = 5;
    let os_width = 10;
    let os_align = Alignment::Left;
    let os_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];
    let os_widget = NetWidget::new(os_len, os_width, os_align, os_cnstrnt);

    // get total size
    let oframe = NetFrame::new(ot_widget, os_widget, om_widget);
    oframe
}

pub fn make_iframe(oframe: NetFrame) -> NetFrame {
    // create inbound title widget
    let it_len = oframe.addrs.len;
    let it_width = 8;
    let it_align = Alignment::Left;
    let it_cnstrnt = vec![Constraint::Percentage(100)];
    let it_widget = NetWidget::new(it_len, it_width, it_align, it_cnstrnt);

    // create inbound addr widget
    let is_len = oframe.addrs.len + oframe.title.len + 1;
    let is_width = 10;
    let is_align = Alignment::Left;
    let is_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];
    let is_widget = NetWidget::new(is_len, is_width, is_align, is_cnstrnt);

    // create inbound msg widget
    let im_len = is_len;
    let im_width = 8;
    let im_align = Alignment::Right;
    let im_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];
    let im_widget = NetWidget::new(im_len, im_width, im_align, im_cnstrnt);

    let iframe = NetFrame::new(it_widget, is_widget, im_widget);
    iframe
}

pub fn make_mframe(iframe: NetFrame) -> NetFrame {
    let mt_len = iframe.title.len + iframe.addrs.len + 1;
    let mt_width = 8;
    let mt_align = Alignment::Left;
    let mt_cnstrnt = vec![Constraint::Percentage(100)];
    let mt_widget = NetWidget::new(mt_len, mt_width, mt_align, mt_cnstrnt);

    // create manual data widget
    let mk_len = mt_len + 1;
    let mk_width = 10;
    let mk_align = Alignment::Left;
    let mk_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];
    let mk_widget = NetWidget::new(mk_len, mk_width, mk_align, mk_cnstrnt);

    // create manual data widget
    let mm_len = mt_len + 1;
    let mm_width = 10;
    let mm_align = Alignment::Left;
    let mm_cnstrnt = vec![Constraint::Percentage(45), Constraint::Percentage(55)];
    let mm_widget = NetWidget::new(mm_len, mm_width, mm_align, mm_cnstrnt);

    let mframe = NetFrame::new(mt_widget.clone(), mk_widget.clone(), mm_widget);
    mframe
}

pub fn ui<B: Backend>(f: &mut Frame<'_, B>, view: View) {
    let oframe = make_oframe();
    draw_outbound(f, view.clone(), oframe.clone());

    let iframe = make_iframe(oframe.clone());
    draw_inbound(f, view.clone(), iframe.clone(), oframe.clone());

    let mframe = make_mframe(iframe.clone());
    draw_manual(f, view.clone(), mframe.clone());

    let top_widget = ListObject::new(oframe, iframe, mframe);

    let list_margin = 2;
    let list_direction = Direction::Horizontal;
    let list_cnstrnts = vec![Constraint::Percentage(50), Constraint::Percentage(50)];

    let prev_len = top_widget.manual.addrs.len;
    draw_list(list_margin, prev_len, list_direction, list_cnstrnts, view.clone(), f);
    //render_info_right(view.clone(), f, slice);
}

//fn render_info_right<B: Backend>(_view: View, f: &mut Frame<'_, B>, slice: Vec<Rect>) {
//    let span = vec![];
//    let graph =
//        Paragraph::new(span).block(Block::default().borders(Borders::ALL)).style(Style::default());
//    f.render_widget(graph, slice[1]);
//}

// Top level widget
#[derive(Clone)]
pub struct ListObject {
    pub outbound: NetFrame,
    pub inbound: NetFrame,
    pub manual: NetFrame,
}

impl ListObject {
    pub fn new(outbound: NetFrame, inbound: NetFrame, manual: NetFrame) -> ListObject {
        ListObject { outbound, inbound, manual }
    }
}

// Middle level widgets
#[derive(Clone)]
pub struct NetFrame {
    pub title: NetWidget,
    pub addrs: NetWidget,
    pub msgs: NetWidget,
}

impl NetFrame {
    pub fn new(title: NetWidget, addrs: NetWidget, msgs: NetWidget) -> NetFrame {
        NetFrame { title, addrs, msgs }
    }
    pub fn get_total_len(self) -> usize {
        let t_len = self.title.len;
        let a_len = self.addrs.len;
        let m_len = self.msgs.len;
        let total_len = t_len + a_len + m_len;
        total_len
    }
}

// Lowest level widgets
#[derive(Clone)]
pub struct NetWidget {
    pub len: usize,
    pub width: usize,
    pub align: Alignment,
    pub cnstrnts: Vec<Constraint>,
}

impl NetWidget {
    pub fn new(len: usize, width: usize, align: Alignment, cnstrnts: Vec<Constraint>) -> NetWidget {
        NetWidget { len, width, align, cnstrnts }
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

fn draw_outbound<B: Backend>(f: &mut Frame<'_, B>, view: View, oframe: NetFrame) {
    let mut titles = Vec::new();
    let mut msgs = Vec::new();
    let mut slots = Vec::new();

    titles.push(Spans::from(Span::styled("Outgoing", Style::default())));
    for id in &view.id_list.node_id {
        match &view.info_list.infos.get(id) {
            Some(connects) => {
                // there is only a single outbound connection so this works
                for slot in &connects.outbound[0].slots {
                    if slot.addr.is_empty() {
                        slots.push(Spans::from(format!("Empty")));
                    } else {
                        slots.push(Spans::from(format!("{}", slot.addr)));
                    }
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

    oframe.title.clone().draw(titles.clone(), f);
    let t_len2 = titles.clone().len();
    oframe.title.clone().update(t_len2);

    oframe.addrs.clone().draw(slots.clone(), f);
    let s_len2 = slots.clone().len();
    oframe.addrs.clone().update(s_len2);

    oframe.msgs.clone().draw(msgs.clone(), f);
    let m_len2 = msgs.clone().len();
    oframe.msgs.clone().update(m_len2);
}

fn draw_inbound<B: Backend>(
    f: &mut Frame<'_, B>,
    view: View,
    iframe: NetFrame,
    outframe: NetFrame,
) {
    let slots_len = outframe.addrs.get_len();
    let mut titles = Vec::new();
    let mut addrs = Vec::new();
    let mut msgs = Vec::new();

    // need to have access to slots_len
    for _i in 1..slots_len {
        titles.push(Spans::from(""));
    }
    titles.push(Spans::from(Span::styled("Incoming", Style::default())));
    for id in &view.id_list.node_id {
        match &view.info_list.infos.get(id) {
            Some(connection) => {
                if !connection.inbound.is_empty() {
                    for connect in &connection.inbound {
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
                                // Do nothing for now
                            }
                        }
                    }
                } else {
                    // Do nothing for now
                }
            }
            None => {
                // This should never happen. TODO: make this an error.
            }
        }
    }

    iframe.title.clone().draw(titles.clone(), f);
    let t_len2 = titles.clone().len();
    iframe.title.clone().update(t_len2);

    iframe.addrs.clone().draw(addrs.clone(), f);
    let s_len2 = addrs.clone().len();
    iframe.addrs.clone().update(s_len2);

    iframe.msgs.clone().draw(msgs.clone(), f);
    let m_len2 = msgs.clone().len();
    iframe.msgs.clone().update(m_len2);
}

fn draw_manual<B: Backend>(f: &mut Frame<'_, B>, view: View, mframe: NetFrame) {
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

    mframe.title.clone().draw(titles.clone(), f);
    let t_len2 = titles.clone().len();
    mframe.title.clone().update(t_len2);

    mframe.addrs.clone().draw(keys.clone(), f);
    let s_len2 = keys.clone().len();
    mframe.addrs.clone().update(s_len2);
}

fn draw_list<B: Backend>(
    margin: u16,
    prev_len: usize,
    direction: Direction,
    cnstrnts: Vec<Constraint>,
    mut view: View,
    f: &mut Frame<'_, B>,
) {
    let mut nodes = Vec::new();

    for id in &view.id_list.node_id {
        let mut lines = vec![Spans::from(id.to_string())];
        // need total_len
        for _i in 1..prev_len {
            lines.push(Spans::from(""));
        }
        let ids = ListItem::new(lines).style(Style::default());
        nodes.push(ids)
    }

    let nodes =
        List::new(nodes).block(Block::default().borders(Borders::ALL)).highlight_symbol(">> ");
    // this is just the box around the list. not the actual list
    let slice =
        Layout::default().direction(direction).margin(margin).constraints(cnstrnts).split(f.size());

    f.render_stateful_widget(nodes, slice[0], &mut view.id_list.state);
}
