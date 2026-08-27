/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use super::long_press_timeout;

use async_lock::Mutex as AsyncMutex;
use async_trait::async_trait;
use atomic_float::AtomicF32;
use darkfi::system::{msleep, CondVar};
use darkfi_serial::{deserialize, Decodable, Encodable, SerialDecodable, SerialEncodable};
use kvdb_overlay::Tree;
use miniquad::{KeyCode, KeyMods, MouseButton, TouchPhase};
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use regex::Regex;
use std::{
    collections::VecDeque,
    io::Cursor,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Weak,
    },
};
use tracing::instrument;
use url::Url;

mod page;
pub use page::FileMessageStatus;
use page::MessageBuffer;

use crate::{
    gfx::{gfxtag, DrawCall, DrawInstruction, Point, Rectangle, RenderApi, Renderer},
    mesh::{Color, MeshBuilder},
    prop::{
        PropertyAtomicGuard, PropertyColor, PropertyFloat32, PropertyRect, PropertyStr,
        PropertyUint32, Role,
    },
    scene::{MethodCallSub, Pimpl, SceneNodePtr, SceneNodeWeak},
    text,
    util::clipboard,
    ExecutorPtr,
};

use super::{DrawUpdate, OnModify, RedrawTrigger, UIObject};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::chatview", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::chatview", $($arg)*); } }

const EPSILON: f32 = 0.001;
const BIG_EPSILON: f32 = 0.05;

/// Mouse must move more than this many pixels while held to count as a drag
/// (which only selects) rather than a click (which toggles).
const SELECT_DRAG_THRESHOLD: f32 = 2.;

/// Finger must stay within this many pixels of the touch-start position to
/// count as "stationary" for long-hold selection on touch screens.
const TOUCH_STATIONARY_THRESHOLD: f32 = 10.;

/// Tracks an in-progress mouse selection gesture so we can distinguish a
/// stationary click (toggles the line) from a drag (only ever selects).
struct SelectDrag {
    down_y: f32,
    was_selected: bool,
    dragged: bool,
}

fn is_zero(x: f32) -> bool {
    x.abs() < EPSILON
}

/// std::cmp::max() doesn't work on f32
#[allow(dead_code)]
fn max(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

#[cfg(feature = "enable-plugin-fud")]
fn get_file_url(text: &String) -> Option<Url> {
    let url_regex = Regex::new(r"fud://[^\s]+").unwrap();
    url_regex.find(text).and_then(|match_| Url::parse(match_.as_str()).ok())
}

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ChatMsg {
    pub nick: String,
    pub text: String,
}

pub type Timestamp = u64;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable, PartialEq)]
pub struct MessageId(pub [u8; 32]);

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for b in &self.0 {
            write!(f, "{b:02x}")?
        }
        Ok(())
    }
}

const PRELOAD_PAGES: usize = 1;

#[derive(Clone)]
struct TouchInfo {
    start_scroll: f32,
    start_y: f32,
    start_instant: std::time::Instant,

    /// Used for flick scrolling
    samples: VecDeque<(std::time::Instant, f32)>,

    last_instant: std::time::Instant,
    last_y: f32,

    /// Selection started?
    is_select_mode: Option<bool>,
}

impl TouchInfo {
    fn new(start_scroll: f32, y: f32) -> Self {
        Self {
            start_scroll,
            start_y: y,
            start_instant: std::time::Instant::now(),
            samples: VecDeque::from([(std::time::Instant::now(), y)]),
            last_instant: std::time::Instant::now(),
            last_y: y,
            is_select_mode: None,
        }
    }

    fn push_sample(&mut self, y: f32) {
        self.samples.push_back((std::time::Instant::now(), y));

        // Now drop all old samples older than 40ms
        while let Some((instant, _)) = self.samples.front() {
            if instant.elapsed().as_micros() <= 40_000 {
                break
            }
            self.samples.pop_front().unwrap();
        }
    }

    fn first_sample(&self) -> Option<(f32, f32)> {
        self.samples.front().map(|(t, s)| (t.elapsed().as_micros() as f32 / 1000., *s))
    }
}

pub type ChatViewPtr = Arc<ChatView>;

/// Transient "Copied link" overlay state. Rendered as a `DrawInstruction::Overlay`
/// in its own draw call so it floats above the link and escapes the message clip rect.
struct LinkToast {
    text_layout: text::TextLayout,
    anchor: Point,
    offset: f32,
    fg_color: Color,
    bg_color: Color,
    font_size: f32,
    padding: f32,
}

impl LinkToast {
    /// Build the overlay draw-instructions (mirroring the edit action menu): a
    /// filled background box with an fg-colored outline and the label centered
    /// inside, drawn above the anchor point (shifted up by `offset`).
    fn build_instrs(&self, renderer: &Renderer) -> Vec<DrawInstruction> {
        let text_w = self.text_layout.width();
        let text_h = self.text_layout.height();
        let pad = self.padding;
        let box_w = text_w + 2. * pad;
        let box_h = self.font_size + 2. * pad;

        // Box bottom sits `offset` above the anchor (negative y is up).
        let pos = Point::new(self.anchor.x, self.anchor.y - self.offset);

        let mut instrs = vec![DrawInstruction::Move(pos)];

        // Box: top at -box_h, bottom at 0 (like an edit action item).
        instrs.push(DrawInstruction::Move(Point::new(0., -box_h)));
        let bg_rect = Rectangle::new(0., 0., box_w, box_h);
        let mut mesh = MeshBuilder::new(gfxtag!("chatview_urltoast_bg"));
        mesh.draw_filled_box(&bg_rect, self.bg_color);
        mesh.draw_outline(&bg_rect, self.fg_color, 1.);
        instrs.push(DrawInstruction::Draw(mesh.alloc(renderer).draw_untextured()));

        // Label: inset horizontally by padding, centered vertically.
        let text_y = (box_h - text_h) / 2.;
        instrs.push(DrawInstruction::Move(Point::new(pad, text_y)));
        let mut txt_instrs =
            text::render_layout(&self.text_layout, renderer, gfxtag!("chatview_urltoast_txt"));
        instrs.append(&mut txt_instrs);

        instrs
    }
}

pub struct ChatView {
    node: SceneNodeWeak,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    renderer: Renderer,
    redraw: RedrawTrigger,
    sg_root: SceneNodePtr,

    tree: Tree,
    msgbuf: AsyncMutex<MessageBuffer>,
    dc_key: u64,

    /// Used for detecting when scrolling view
    mouse_pos: SyncMutex<Point>,
    /// Touch scrolling
    touch_info: SyncMutex<Option<TouchInfo>>,
    touch_is_active: AtomicBool,

    rect: PropertyRect,
    scroll: PropertyFloat32,
    z_index: PropertyUint32,
    priority: PropertyUint32,

    scroll_start_accel: PropertyFloat32,
    scroll_resist: PropertyFloat32,
    key_scroll_speed: PropertyFloat32,

    /// Scroll accel
    motion_cv: Arc<CondVar>,
    speed: AtomicF32,

    mouse_btn_held: AtomicBool,

    /// In-progress mouse selection gesture (set on left button down).
    select_drag: SyncMutex<Option<SelectDrag>>,

    /// Last reported selection state, used to emit `select_changed` only on
    /// transitions between having and not having a selection.
    select_active: AtomicBool,

    /// Triggers the background loading task to wake up.
    /// We use this since there should only ever be a single bg task loading.
    bgload_cv: Arc<CondVar>,

    /// We use it when we re-eval rect when its changed via property.
    parent_rect: SyncMutex<Option<Rectangle>>,

    /// window scale (for building the toast text layout)
    window_scale: PropertyFloat32,

    /// "Copied link" overlay styling/content
    url_copy_text: PropertyStr,
    url_copy_fg_color: PropertyColor,
    url_copy_bg_color: PropertyColor,
    url_copy_font_size: PropertyFloat32,
    url_copy_padding: PropertyFloat32,
    url_copy_offset: PropertyFloat32,
    url_copy_duration: PropertyFloat32,

    /// Active copy-link overlay (None when hidden).
    link_toast: SyncMutex<Option<LinkToast>>,
    /// Re-arm counter so a stale dismiss task won't clear a newer toast.
    toast_version: AtomicU32,

    /// Re-arm counter so a stale long-press timer won't fire for a newer touch.
    touch_hold_version: AtomicU32,

    /// Weak self-reference so handlers can spawn detached tasks.
    me: Weak<Self>,
    ex: ExecutorPtr,
}

impl ChatView {
    pub async fn new(
        node: SceneNodeWeak,
        tree: Tree,
        window_scale: PropertyFloat32,
        renderer: Renderer,
        redraw: RedrawTrigger,
        sg_root: SceneNodePtr,
        ex: ExecutorPtr,
    ) -> Pimpl {
        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let scroll = PropertyFloat32::wrap(node_ref, Role::Internal, "scroll", 0).unwrap();
        let font_size = PropertyFloat32::wrap(node_ref, Role::Internal, "font_size", 0).unwrap();
        let timestamp_font_size =
            PropertyFloat32::wrap(node_ref, Role::Internal, "timestamp_font_size", 0).unwrap();
        let timestamp_width =
            PropertyFloat32::wrap(node_ref, Role::Internal, "timestamp_width", 0).unwrap();
        let line_height =
            PropertyFloat32::wrap(node_ref, Role::Internal, "line_height", 0).unwrap();
        let message_spacing =
            PropertyFloat32::wrap(node_ref, Role::Internal, "message_spacing", 0).unwrap();
        let baseline = PropertyFloat32::wrap(node_ref, Role::Internal, "baseline", 0).unwrap();
        let timestamp_color =
            PropertyColor::wrap(node_ref, Role::Internal, "timestamp_color").unwrap();
        let text_color = PropertyColor::wrap(node_ref, Role::Internal, "text_color").unwrap();
        let action_text_color =
            PropertyColor::wrap(node_ref, Role::Internal, "action_text_color").unwrap();
        let url_text_color =
            PropertyColor::wrap(node_ref, Role::Internal, "url_text_color").unwrap();
        let url_bg_color = PropertyColor::wrap(node_ref, Role::Internal, "url_bg_color").unwrap();
        let url_bg_border_size =
            PropertyFloat32::wrap(node_ref, Role::Internal, "url_bg_border_size", 0).unwrap();
        let url_bg_border_color =
            PropertyColor::wrap(node_ref, Role::Internal, "url_bg_border_color").unwrap();
        let url_copy_text =
            PropertyStr::wrap(node_ref, Role::Internal, "url_copy_text", 0).unwrap();
        let url_copy_fg_color =
            PropertyColor::wrap(node_ref, Role::Internal, "url_copy_fg_color").unwrap();
        let url_copy_bg_color =
            PropertyColor::wrap(node_ref, Role::Internal, "url_copy_bg_color").unwrap();
        let url_copy_font_size =
            PropertyFloat32::wrap(node_ref, Role::Internal, "url_copy_font_size", 0).unwrap();
        let url_copy_padding =
            PropertyFloat32::wrap(node_ref, Role::Internal, "url_copy_padding", 0).unwrap();
        let url_copy_offset =
            PropertyFloat32::wrap(node_ref, Role::Internal, "url_copy_offset", 0).unwrap();
        let url_copy_duration =
            PropertyFloat32::wrap(node_ref, Role::Internal, "url_copy_duration", 0).unwrap();
        let nick_colors = node_ref.get_property("nick_colors").expect("ChatView::nick_colors");
        let hi_bg_color = PropertyColor::wrap(node_ref, Role::Internal, "hi_bg_color").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        // Unused currently
        //let debug = PropertyBool::wrap(node_ref, Role::Internal, "debug", 0).unwrap();

        let scroll_start_accel =
            PropertyFloat32::wrap(node_ref, Role::Internal, "scroll_start_accel", 0).unwrap();
        let scroll_resist =
            PropertyFloat32::wrap(node_ref, Role::Internal, "scroll_resist", 0).unwrap();
        let key_scroll_speed =
            PropertyFloat32::wrap(node_ref, Role::Internal, "key_scroll_speed", 0).unwrap();

        let motion_cv = Arc::new(CondVar::new());
        let bgload_cv = Arc::new(CondVar::new());

        let self_ = Arc::new_cyclic(|me| Self {
            node: node.clone(),
            tasks: SyncMutex::new(vec![]),
            renderer: renderer.clone(),
            redraw,
            sg_root,

            tree,
            msgbuf: AsyncMutex::new(MessageBuffer::new(
                font_size,
                timestamp_font_size,
                timestamp_width,
                line_height,
                message_spacing,
                baseline,
                timestamp_color,
                text_color,
                action_text_color,
                url_text_color,
                url_bg_color,
                url_bg_border_size,
                url_bg_border_color,
                nick_colors,
                hi_bg_color,
                window_scale.clone(),
                renderer,
            )),
            dc_key: OsRng.gen(),

            mouse_pos: SyncMutex::new(Point::from([0., 0.])),
            touch_info: SyncMutex::new(None),
            touch_is_active: AtomicBool::new(false),

            rect,
            scroll,
            z_index,
            priority,

            scroll_start_accel,
            scroll_resist,
            key_scroll_speed,

            motion_cv,
            speed: AtomicF32::new(0.),

            mouse_btn_held: AtomicBool::new(false),

            select_drag: SyncMutex::new(None),

            select_active: AtomicBool::new(false),

            bgload_cv,

            parent_rect: SyncMutex::new(None),

            window_scale,
            url_copy_text,
            url_copy_fg_color,
            url_copy_bg_color,
            url_copy_font_size,
            url_copy_padding,
            url_copy_offset,
            url_copy_duration,
            link_toast: SyncMutex::new(None),
            toast_version: AtomicU32::new(0),
            touch_hold_version: AtomicU32::new(0),
            me: me.clone(),
            ex,
        });
        Pimpl::ChatView(self_)
    }

    async fn process_insert_line_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("Event relayer closed");
            return false
        };

        t!("method called: insert_line({method_call:?})");
        assert!(method_call.send_res.is_none());

        fn decode_data(data: &[u8]) -> std::io::Result<(Timestamp, MessageId, String, String)> {
            let mut cur = Cursor::new(&data);
            let timestamp = Timestamp::decode(&mut cur)?;
            let msg_id = MessageId::decode(&mut cur)?;
            let nick = String::decode(&mut cur)?;
            let text = String::decode(&mut cur)?;
            Ok((timestamp, msg_id, nick, text))
        }

        let Ok((timestamp, msg_id, nick, text)) = decode_data(&method_call.data) else {
            error!(target: "ui::chatview", "insert_line() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before insert_line_method_task was stopped!");
        };

        self_.handle_insert_line(timestamp, msg_id, nick, text).await;
        true
    }
    async fn process_insert_unconf_line_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("Event relayer closed");
            return false
        };

        t!("method called: insert_unconf_line({method_call:?})");
        assert!(method_call.send_res.is_none());

        fn decode_data(data: &[u8]) -> std::io::Result<(Timestamp, MessageId, String, String)> {
            let mut cur = Cursor::new(&data);
            let timestamp = Timestamp::decode(&mut cur)?;
            let msg_id = MessageId::decode(&mut cur)?;
            let nick = String::decode(&mut cur)?;
            let text = String::decode(&mut cur)?;
            Ok((timestamp, msg_id, nick, text))
        }

        let Ok((timestamp, msg_id, nick, text)) = decode_data(&method_call.data) else {
            error!(target: "ui::chatview", "insert_unconf_line() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before touch_task was stopped!");
        };

        self_.handle_insert_unconf_line(timestamp, msg_id, nick, text).await;
        true
    }
    async fn process_set_file_status_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("Event relayer closed");
            return false
        };

        t!("method called: set_file_status({method_call:?})");
        assert!(method_call.send_res.is_none());

        fn decode_data(data: &[u8]) -> std::io::Result<(Url, FileMessageStatus)> {
            let mut cur = Cursor::new(&data);
            let url = Url::decode(&mut cur)?;
            let file_status = FileMessageStatus::decode(&mut cur)?;
            Ok((url, file_status))
        }

        let Ok((url, file_status)) = decode_data(&method_call.data) else {
            error!(target: "ui::chatview", "set_file_status() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before set_file_status_task was stopped!");
        };

        let mut msgbuf = self_.msgbuf.lock().await;
        msgbuf.update_file_status(&url, &file_status);
        msgbuf.adjust_params();
        drop(msgbuf);
        self_.redraw.trigger();

        true
    }

    async fn process_copy_select_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("Event relayer closed");
            return false
        };

        t!("method called: copy_select({method_call:?})");
        assert!(method_call.send_res.is_none());
        assert!(method_call.data.is_empty());

        let Some(self_) = me.upgrade() else {
            panic!("self destroyed before copy_select_method_task was stopped!");
        };

        self_.handle_copy_select().await;
        true
    }

    async fn process_unselect_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("Event relayer closed");
            return false
        };

        t!("method called: unselect({method_call:?})");
        assert!(method_call.send_res.is_none());
        assert!(method_call.data.is_empty());

        let Some(self_) = me.upgrade() else {
            panic!("self destroyed before unselect_method_task was stopped!");
        };

        self_.handle_unselect().await;
        true
    }

    fn to_msgbuf_pos(&self, pos: Point) -> Point {
        let mut x = pos.x;
        let mut y = pos.y;

        let rect = self.rect.get();

        x -= rect.x;
        y -= rect.y;
        let scroll = self.scroll.get();
        y = rect.h - y + scroll;

        Point::new(x, y)
    }

    /// Mark line as selected
    #[instrument(target = "ui::chatview")]
    async fn select_line(&self, mut y: f32) {
        // The cursor is inside the rect. We just have to find which line it clicked.
        let rect = self.rect.get();

        // y coord within widget's screen rect
        y -= rect.y;
        // The scroll is the position of the bottom of the rect on screen
        let scroll = self.scroll.get();
        // Now what is its distance from the absolute bottom
        y = rect.h - y + scroll;

        let mut msgbuf = self.msgbuf.lock().await;
        let had = msgbuf.has_selection();
        msgbuf.select_line(&rect, y).await;
        let has = msgbuf.has_selection();
        drop(msgbuf);

        self.redraw.trigger();
        self.notify_select_changed(has, had).await;
    }

    /// Mark line as deselected
    #[instrument(target = "ui::chatview")]
    async fn deselect_line(&self, mut y: f32) {
        let rect = self.rect.get();
        y -= rect.y;
        let scroll = self.scroll.get();
        y = rect.h - y + scroll;

        let mut msgbuf = self.msgbuf.lock().await;
        let had = msgbuf.has_selection();
        msgbuf.deselect_line(&rect, y).await;
        let has = msgbuf.has_selection();
        drop(msgbuf);

        self.redraw.trigger();
        self.notify_select_changed(has, had).await;
    }

    /// Query whether the line under screen y is currently selected.
    async fn is_line_selected(&self, screen_y: f32) -> bool {
        let rect = self.rect.get();
        let y = self.to_msgbuf_pos(Point::new(0., screen_y)).y;
        let mut msgbuf = self.msgbuf.lock().await;
        msgbuf.is_line_selected(&rect, y).await
    }

    /// Emit `select_changed(true/false)` whenever the presence of any selected
    /// line transitions. `has` is the current state, `had` the previous one.
    async fn notify_select_changed(&self, has: bool, had: bool) {
        if has == had {
            return
        }
        self.select_active.store(has, Ordering::Relaxed);
        let Some(node_ref) = self.node.upgrade() else { return };
        let mut data = vec![];
        has.encode(&mut data).unwrap();
        let _ = node_ref.trigger("select_changed", data).await;
    }

    /// Copy the currently selected messages' text to the clipboard.
    async fn handle_copy_select(&self) {
        let msgbuf = self.msgbuf.lock().await;
        let text = msgbuf.selected_text();
        drop(msgbuf);
        if !text.is_empty() {
            t!("handle_copy_select() [text={text}]");
            clipboard::set(&text);
        }
        // Also unselect the existing selected text
        self.handle_unselect().await
    }

    /// Deselect every selected message and redraw.
    async fn handle_unselect(&self) {
        let mut msgbuf = self.msgbuf.lock().await;
        let had = msgbuf.has_selection();
        msgbuf.unselect_all();
        let has = msgbuf.has_selection();
        drop(msgbuf);

        self.redraw.trigger();
        self.notify_select_changed(has, had).await;
    }

    fn end_touch_phase(&self, touch_y: f32) {
        // Cancel any pending long-press timer.
        self.touch_hold_version.fetch_add(1, Ordering::SeqCst);

        // Now calculate scroll acceleration
        let touch_info = std::mem::replace(&mut *self.touch_info.lock(), None);
        let Some(touch_info) = &touch_info else { return };

        self.touch_is_active.store(false, Ordering::Relaxed);

        // No scroll accel when selection was active.
        if touch_info.is_select_mode == Some(true) {
            return
        }

        let Some((time, sample_y)) = touch_info.first_sample() else { return };
        let dist = touch_y - sample_y;

        // Ignore sub-ms events
        if time < 1. {
            error!(target: "ui::chatview", "Received a sub-ms touch event!");
            return
        }

        //let speed = dist / time;
        //self.speed.fetch_add(speed, Ordering::Relaxed);
        //debug!(target: "ui::chatview", "speed = {dist} / {time} = {speed}");

        let accel = self.scroll_start_accel.get() * dist / time;
        let touch_time = touch_info.start_instant.elapsed();
        t!("accel = {dist} / {time} = {accel},  touch = {touch_time:?}");
        self.speed.fetch_add(accel, Ordering::Relaxed);
        self.motion_cv.notify();
    }

    async fn add_line_to_db(
        &self,
        timest: Timestamp,
        msg_id: &MessageId,
        nick: &str,
        text: &str,
    ) -> bool {
        assert!(timest > 6047051717);
        let timest = timest.to_be_bytes();
        assert_eq!(timest.len(), 8);
        let mut key = [0u8; 8 + 32];
        key[..8].clone_from_slice(&timest);
        key[8..].clone_from_slice(&msg_id.0);

        // When does this return Err?
        let contains_key = self.tree.contains_key(&key);
        if contains_key.is_err() || contains_key.unwrap() {
            // Already exists
            return false
        }

        let msg = ChatMsg { nick: nick.to_string(), text: text.to_string() };
        let mut val = vec![];
        msg.encode(&mut val).unwrap();

        self.tree.insert(&key, &val).unwrap();
        true
    }
    #[instrument(target = "ui::chatview")]
    pub async fn handle_insert_line(
        &self,
        timest: Timestamp,
        msg_id: MessageId,
        nick: String,
        text: String,
    ) {
        // Lock message buffer so background loader doesn't load the message as soon as it's
        // inserted into the DB.
        let mut msgbuf = self.msgbuf.lock().await;

        if !self.add_line_to_db(timest, &msg_id, &nick, &text).await {
            // Already exists so bail
            t!("duplicate msg so bailing");
            return
        }

        // Add message to page
        if msgbuf.mark_confirmed(&msg_id) {
            // Message already exists. Which means it must be an unconfirmed sent message.
            // Mark it as confirmed.
            t!("Mark sent message as confirmed");
        } else {
            t!("Inserting new message");

            // Insert the privmsg since it doesn't already exist
            let privmsg = msgbuf
                .insert_privmsg(timest, msg_id.clone(), nick.clone(), text.clone(), self.rect.get())
                .await;
            if privmsg.is_none() {
                // Not visible so no need to redraw
                return
            }

            #[cfg(feature = "enable-plugin-fud")]
            {
                if let Some(url) = get_file_url(&text) {
                    let _ =
                        msgbuf.insert_filemsg(self.node.clone(), timest, msg_id, nick, url.clone());

                    let node_ref = self.node.upgrade().unwrap();
                    let mut data = vec![];
                    url.encode(&mut data).unwrap();
                    let _ = node_ref.trigger("fileurl_detected", data).await;
                }
            }
        }

        drop(msgbuf);
        self.redraw.trigger();
        self.bgload_cv.notify();
    }
    #[instrument(target = "ui::chatview")]
    async fn handle_insert_unconf_line(
        &self,
        timest: Timestamp,
        msg_id: MessageId,
        nick: String,
        text: String,
    ) {
        // We don't add unconfirmed lines to the db. Maybe we should?

        // Add message to page
        let mut msgbuf = self.msgbuf.lock().await;
        let Some(privmsg) =
            msgbuf.insert_privmsg(timest, msg_id, nick, text, self.rect.get()).await
        else {
            return
        };
        privmsg.confirmed = false;
        drop(msgbuf);
        self.redraw.trigger();
        self.bgload_cv.notify();
    }

    /// Signal to begin scrolling
    fn start_scroll(&self, y: f32) {
        self.speed.fetch_add(y * self.scroll_start_accel.get(), Ordering::Relaxed);
        self.motion_cv.notify();
    }

    async fn handle_movement(&self) {
        // We need to fix this impl because it depends very much on the speed of the device
        // that it's running on.
        // Look into optimizing scrollview() so scrolling is smooth.
        // We could use skiplists to avoid looping from the very bottom.
        // So index 1 in the skiplist advances to 100px up... (or however much multiplier)
        loop {
            msleep(10).await;

            if self.touch_is_active.load(Ordering::Relaxed) {
                return
            }

            let mut speed = self.speed.load(Ordering::Relaxed);

            // Apply constant decel to speed
            speed *= self.scroll_resist.get();
            if speed.abs() < BIG_EPSILON {
                speed = 0.;
            }
            self.speed.store(speed, Ordering::Relaxed);

            // Finished
            if is_zero(speed) {
                return
            }

            let scroll = self.scroll.get() + speed;
            let atom = &mut self.redraw.make_guard(gfxtag!("ChatView::motion_task"));
            let dist = self.scrollview(scroll, atom).await;

            // We reached the end so just stop
            if is_zero(dist) {
                self.speed.store(0., Ordering::Relaxed);
                return
            }
        }
    }

    async fn handle_bgload(&self) {
        // Do we need to load some more?
        let scroll = self.scroll.get();
        let rect = self.rect.get();
        let top = scroll + rect.h;

        let preload_height = PRELOAD_PAGES as f32 * rect.h;

        let mut msgbuf = self.msgbuf.lock().await;

        let total_height = msgbuf.calc_total_height(&rect).await;
        if total_height > top + preload_height {
            // Nothing to do here
            //t!("bgloader: buffer is sufficient [trace_id={trace_id}]");
            return
        }

        // Keep loading until this is below 0
        let mut remaining_load_height = top + preload_height - total_height;
        let mut remaining_visible = top - total_height;
        //t!("bgloader: remaining px = {remaining_load_height}, remaining_visible={remaining_visible} [trace_id={trace_id}]");

        // Get the current earliest timestamp
        let iter = match msgbuf.oldest_timestamp() {
            Some(oldest_timest) => {
                // iterate from there
                //t!("preloading from {oldest_timest} [trace_id={trace_id}]");
                let timest = (oldest_timest - 1).to_be_bytes();
                let mut key = [0u8; 8 + 32];
                key[..8].clone_from_slice(&timest);

                let iter = self.tree.range(..key).rev();
                iter
            }
            None => {
                //t!("initial load [trace_id={trace_id}]");
                self.tree.iter().rev()
            }
        };

        let mut do_redraw = false;
        for entry in iter {
            let Ok((k, v)) = entry else { break };
            assert_eq!(k.len(), 8 + 32);
            let timest_bytes: [u8; 8] = k[..8].try_into().unwrap();
            let msg_id = MessageId(k[8..].try_into().unwrap());
            let timest = Timestamp::from_be_bytes(timest_bytes);
            let chatmsg: ChatMsg = deserialize(&v).unwrap();

            //t!("{timest:?} {chatmsg:?} [trace_id={trace_id}]");
            let msg_height = msgbuf
                .push_privmsg(
                    timest,
                    msg_id.clone(),
                    chatmsg.nick.clone(),
                    chatmsg.text.clone(),
                    &rect,
                )
                .await;

            #[cfg(feature = "enable-plugin-fud")]
            {
                if let Some(url) = get_file_url(&chatmsg.text) {
                    let _ = msgbuf.insert_filemsg(
                        self.node.clone(),
                        timest,
                        msg_id,
                        chatmsg.nick.clone(),
                        url.clone(),
                    );

                    let node_ref = self.node.upgrade().unwrap();
                    let mut data = vec![];
                    url.encode(&mut data).unwrap();
                    let _ = node_ref.trigger("fileurl_detected", data).await;
                }
            }

            remaining_load_height -= msg_height;
            if remaining_load_height <= 0. {
                break
            }

            // Do this once at the end rather than continuously redrawing
            if remaining_visible > 0. {
                do_redraw = true;
            }
            remaining_visible -= msg_height;
        }
        if do_redraw {
            self.redraw.trigger();
        }
    }

    async fn scrollview(&self, mut scroll: f32, atom: &mut PropertyAtomicGuard) -> f32 {
        let old_scroll = self.scroll.get();

        let rect = self.rect.get();

        let mut msgbuf = self.msgbuf.lock().await;

        // 1/3 of time spent here  ~1.5ms
        if let Some(new_scroll) = self.adjust_scroll(&mut msgbuf, scroll, rect.h).await {
            scroll = new_scroll;
        }
        drop(msgbuf);

        self.scroll.set(atom, scroll);
        self.bgload_cv.notify();

        scroll - old_scroll
    }

    /// Adjusts a proposed scroll value to clamp it within range. It will load pages until we
    /// either run out or we have enough, then checks scroll is within range.
    /// Returns None if the value is within range.
    async fn adjust_scroll(
        &self,
        msgbuf: &mut MessageBuffer,
        mut scroll: f32,
        rect_h: f32,
    ) -> Option<f32> {
        // We still wish to preload pages to fill the screen, so we just adjust it up to 0.
        //let nonneg_scroll = max(scroll, 0.);

        if scroll < 0. {
            return Some(0.)
        }

        let total_height = msgbuf.calc_total_height(&self.rect.get()).await;
        let max_allowed_scroll = if total_height > rect_h { total_height - rect_h } else { 0. };

        if scroll > max_allowed_scroll {
            scroll = max_allowed_scroll;
            assert!(scroll >= 0.);
            return Some(scroll)
        }

        // Unchanged
        None
    }

    /// Returns draw calls for drawing
    async fn get_meshes(
        &self,
        msgbuf: &mut MessageBuffer,
        rect: &Rectangle,
    ) -> Vec<DrawInstruction> {
        let scroll = self.scroll.get();
        //let total_height = msgbuf.calc_total_height().await;

        // Use this to start from the top
        //let start_pos = if total_height < rect.h { total_height } else { rect.h };
        // We start from the bottom though
        let start_pos = rect.h;

        let mut instrs = vec![];
        //let mut old_drawmesh = vec![];

        let meshes = msgbuf.gen_meshes(rect, scroll).await;

        for (y_pos, mut minstrs) in meshes {
            // Apply scroll and scissor
            // We use the scissor for scrolling
            // Because we use the scissor, our actual rect is now rect instead of parent_rect
            let off_x = 0.;
            // This calc decides whether scroll is in terms of pages or pixels
            let off_y = scroll + start_pos - y_pos;
            let pos = Point::from([off_x, off_y]);

            instrs.push(DrawInstruction::SetPos(pos));
            instrs.append(&mut minstrs);
        }

        instrs
    }

    /// Build the copy-link overlay instruction if a toast is currently shown.
    fn toast_instr(&self) -> Option<DrawInstruction> {
        let toast = self.link_toast.lock();
        let toast = toast.as_ref()?;
        Some(DrawInstruction::Overlay(toast.build_instrs(&self.renderer)))
    }

    /// Copy `url` to the clipboard and show the "Copied link" overlay above `anchor`
    /// (chatview-local coords) for `url_copy_duration` seconds. Re-arms on repeat.
    /// The overlay is emitted inline by the draw pass (so it inherits the
    /// chatview's position), hence we trigger a redraw on show/hide.
    async fn show_toast(&self, url: &str, anchor: Point) {
        clipboard::set(url);

        let text = self.url_copy_text.get();
        let fg = self.url_copy_fg_color.get();
        let bg = self.url_copy_bg_color.get();
        let font_size = self.url_copy_font_size.get();
        let pad = self.url_copy_padding.get();
        let offset = self.url_copy_offset.get();
        let window_scale = self.window_scale.get();

        let text_layout = text::make_layout(&text, fg, font_size, 0., window_scale, None, &[]);
        let toast = LinkToast {
            text_layout,
            anchor,
            offset,
            fg_color: fg,
            bg_color: bg,
            font_size,
            padding: pad,
        };
        *self.link_toast.lock() = Some(toast);
        self.redraw.trigger();

        // (Re)arm the dismiss task. A stale task won't clear a newer toast (version check).
        let version = self.toast_version.fetch_add(1, Ordering::SeqCst) + 1;
        let duration = self.url_copy_duration.get();
        let me = self.me.clone();
        let ex = self.ex.clone();
        ex.spawn(async move {
            msleep((duration * 1000.) as u64).await;
            let Some(self_) = me.upgrade() else { return };
            if self_.toast_version.load(Ordering::SeqCst) == version {
                *self_.link_toast.lock() = None;
                self_.redraw.trigger();
            }
        })
        .detach();
    }

    /// Called by the long-press timer after `select_hold_time` elapses.
    /// If the finger is still down and within the stationary threshold, starts
    /// text selection (or copies the URL if the finger is on one).
    async fn long_hold_fire(&self, version: u32, start_pos: Point) {
        // Cancelled by Ended/Cancelled or a newer touch.
        if self.touch_hold_version.load(Ordering::SeqCst) != version {
            return
        }

        // Touch ended or still undecided?
        let (start_y, last_y) = {
            let touch_info = self.touch_info.lock();
            let Some(ti) = &*touch_info else { return };
            (ti.start_y, ti.last_y)
        };

        // Finger moved beyond the stationary threshold — it's a scroll.
        if (last_y - start_y).abs() > TOUCH_STATIONARY_THRESHOLD {
            if let Some(ti) = &mut *self.touch_info.lock() {
                ti.is_select_mode = Some(false);
            }
            return
        }

        let rect = self.rect.get();

        // URL under the finger takes priority: copy it, don't select.
        let msgbuf_pos = self.to_msgbuf_pos(start_pos);
        let mut msgbuf = self.msgbuf.lock().await;
        let on_url = msgbuf.url_at(&rect, msgbuf_pos.x, msgbuf_pos.y).await;
        drop(msgbuf);

        if let Some(url) = on_url {
            self.show_toast(&url, start_pos - rect.pos()).await;
            if let Some(ti) = &mut *self.touch_info.lock() {
                ti.is_select_mode = Some(false);
            }
            return
        }

        // Not on a URL: start text selection.
        if let Some(ti) = &mut *self.touch_info.lock() {
            ti.is_select_mode = Some(true);
        }
        self.select_line(start_pos.y).await;
    }
}

#[async_trait]
impl UIObject for ChatView {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = self.me.clone();

        let node_ref = &self.node.upgrade().unwrap();

        let method_sub = node_ref.subscribe_method_call("insert_line").unwrap();
        let me2 = me.clone();
        let insert_line_method_task =
            ex.spawn(
                async move { while Self::process_insert_line_method(&me2, &method_sub).await {} },
            );

        let method_sub = node_ref.subscribe_method_call("insert_unconf_line").unwrap();
        let me2 = me.clone();
        let insert_unconf_line_method_task = ex.spawn(async move {
            while Self::process_insert_unconf_line_method(&me2, &method_sub).await {}
        });

        let me2 = me.clone();
        let cv = self.motion_cv.clone();
        let motion_task = ex.spawn(async move {
            loop {
                cv.wait().await;
                let Some(self_) = me2.upgrade() else {
                    // Should not happen
                    panic!("self destroyed before motion_task was stopped!");
                };
                self_.handle_movement().await;
                cv.reset();
            }
        });

        let me2 = me.clone();
        let cv = self.bgload_cv.clone();
        let bgload_task = ex.spawn(async move {
            loop {
                cv.wait().await;
                let Some(self_) = me2.upgrade() else {
                    // Should not happen
                    panic!("self destroyed before bgload_task was stopped!");
                };
                self_.handle_bgload().await;
                cv.reset();
            }
        });

        let method_sub = node_ref.subscribe_method_call("set_file_status").unwrap();
        let me2 = me.clone();
        let set_file_status_method_task = ex.spawn(async move {
            while Self::process_set_file_status_method(&me2, &method_sub).await {}
        });

        let method_sub = node_ref.subscribe_method_call("copy_select").unwrap();
        let me2 = me.clone();
        let copy_select_method_task =
            ex.spawn(
                async move { while Self::process_copy_select_method(&me2, &method_sub).await {} },
            );

        let method_sub = node_ref.subscribe_method_call("unselect").unwrap();
        let me2 = me.clone();
        let unselect_method_task = ex
            .spawn(async move { while Self::process_unselect_method(&me2, &method_sub).await {} });

        let mut on_modify = OnModify::new(ex, self.node.clone(), me.clone());

        on_modify.when_change_external(self.rect.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });

        let mut tasks = vec![
            insert_line_method_task,
            insert_unconf_line_method_task,
            motion_task,
            bgload_task,
            set_file_status_method_task,
            copy_select_method_task,
            unselect_method_task,
        ];
        tasks.append(&mut on_modify.tasks);

        *self.tasks.lock() = tasks;
    }

    fn stop(&self) {
        self.tasks.lock().clear();
        *self.parent_rect.lock() = None;
        // Clear mesh caches
        self.msgbuf.lock_blocking().clear();
    }

    #[instrument(target = "ui::chatview")]
    async fn draw(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        *self.parent_rect.lock() = Some(parent_rect.clone());

        // Rect property is its own memo: compare before/after eval.
        let prev_rect = self.rect.get();
        self.rect.eval(atom, &parent_rect).ok()?;
        let rect = self.rect.get();
        let rect_changed = rect != prev_rect;

        let mut msgbuf = self.msgbuf.lock().await;
        let scale_changed = msgbuf.adjust_window_scale();
        // Mesh caches hold epoch-scoped GPU resources; drop them after a
        // UI restart so messages are rebuilt against the new epoch.
        let epoch_changed = msgbuf.epoch_changed();
        if rect_changed || scale_changed || epoch_changed {
            msgbuf.clear_meshes();
        }

        let scroll = self.scroll.get();
        if let Some(scroll) = self.adjust_scroll(&mut msgbuf, scroll, rect.h).await {
            self.scroll.set(atom, scroll);
        }

        // We may need to load more messages since the screen size has changed.
        // Now we have updated all the values so it's safe to wake up here.
        self.bgload_cv.notify();

        let mut mesh_instrs = self.get_meshes(&mut msgbuf, &rect).await;
        drop(msgbuf);

        let mut instrs = vec![DrawInstruction::ApplyView(rect)];
        instrs.append(&mut mesh_instrs);
        if let Some(t) = self.toast_instr() {
            instrs.push(t);
        }

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                DrawCall::new(instrs, vec![], self.z_index.get(), "chatview"),
            )],
        })
    }

    async fn handle_key_down(&self, key: KeyCode, _mods: KeyMods, repeat: bool) -> bool {
        if repeat {
            return false
        }

        match key {
            KeyCode::PageUp => {
                self.start_scroll(1. * self.key_scroll_speed.get());
                return true
            }
            KeyCode::PageDown => {
                self.start_scroll(-1. * self.key_scroll_speed.get());
                return true
            }
            _ => {}
        }

        false
    }

    async fn handle_mouse_btn_down(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        let rect = self.rect.get();

        if rect.contains(mouse_pos) {
            let mut msgbuf = self.msgbuf.lock().await;
            let msgbuf_pos = self.to_msgbuf_pos(mouse_pos);
            if let Some((msg, msg_top)) = msgbuf.get_line(&rect, msgbuf_pos.y).await {
                if msg
                    .handle_mouse_btn_down(btn, Point::new(msgbuf_pos.x, msg_top - msgbuf_pos.y))
                    .await
                {
                    return true
                }
            }
        }

        // Right-click on a URL: copy it and show the "Copied link" overlay.
        if btn == MouseButton::Right && rect.contains(mouse_pos) {
            let msgbuf_pos = self.to_msgbuf_pos(mouse_pos);
            let mut msgbuf = self.msgbuf.lock().await;
            if let Some(url) = msgbuf.url_at(&rect, msgbuf_pos.x, msgbuf_pos.y).await {
                drop(msgbuf);
                self.show_toast(&url, mouse_pos - rect.pos()).await;
                return true
            }
        }

        // Left-click on a URL: consume the press so no selection starts.
        // The URL itself is opened on button-up via the message dispatch.
        if btn == MouseButton::Left && rect.contains(mouse_pos) {
            let msgbuf_pos = self.to_msgbuf_pos(mouse_pos);
            let mut msgbuf = self.msgbuf.lock().await;
            if msgbuf.url_at(&rect, msgbuf_pos.x, msgbuf_pos.y).await.is_some() {
                return true
            }
        }

        if btn != MouseButton::Left {
            return false
        }

        if !rect.contains(mouse_pos) {
            return false
        }

        // Query whether the clicked line is already selected. We select
        // immediately only if it wasn't (for instant feedback). If it was
        // already selected we leave it and let handle_mouse_btn_up decide:
        // a stationary click toggles it off, but a drag keeps it selected.
        let was_selected = self.is_line_selected(mouse_pos.y).await;
        if !was_selected {
            self.select_line(mouse_pos.y).await;
        }
        *self.select_drag.lock() =
            Some(SelectDrag { down_y: mouse_pos.y, was_selected, dragged: false });
        self.mouse_btn_held.store(true, Ordering::Relaxed);
        true
    }

    async fn handle_mouse_btn_up(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        t!("handle_mouse_btn_up({btn:?}, {mouse_pos:?})");

        let rect = self.rect.get();
        if rect.contains(mouse_pos) {
            let mut msgbuf = self.msgbuf.lock().await;
            let msgbuf_pos = self.to_msgbuf_pos(mouse_pos);
            if let Some((msg, msg_top)) = msgbuf.get_line(&rect, msgbuf_pos.y).await {
                if msg
                    .handle_mouse_btn_up(btn, Point::new(msgbuf_pos.x, msg_top - msgbuf_pos.y))
                    .await
                {
                    return true
                }
            }
        }

        if btn != MouseButton::Left {
            return false
        }

        self.mouse_btn_held.store(false, Ordering::Relaxed);

        // A stationary click on an already-selected line deselects it. A drag
        // (or a click on an unselected line) leaves selection as-is.
        let drag = self.select_drag.lock().take();
        if let Some(d) = drag {
            if !d.dragged && d.was_selected {
                self.deselect_line(mouse_pos.y).await;
            }
        }

        false
    }

    async fn handle_mouse_move(&self, mouse_pos: Point) -> bool {
        //t!("handle_mouse_move({mouse_pos:?})");

        // We store the mouse pos for use in handle_mouse_wheel()
        *self.mouse_pos.lock() = mouse_pos.clone();

        if !self.mouse_btn_held.load(Ordering::Relaxed) {
            return false
        }

        let rect = self.rect.get();
        if !rect.contains(mouse_pos) {
            return false
        }

        // Dragging only ever selects. Once the mouse moves past the
        // threshold we latch `dragged` so the upcoming mouse-up won't
        // treat this as a toggling click.
        let dragged = {
            let mut drag = self.select_drag.lock();
            if let Some(d) = drag.as_mut() {
                if (mouse_pos.y - d.down_y).abs() > SELECT_DRAG_THRESHOLD {
                    d.dragged = true;
                }
                d.dragged
            } else {
                false
            }
        };
        if dragged {
            self.select_line(mouse_pos.y).await;
        }

        let mut msgbuf = self.msgbuf.lock().await;
        let msgbuf_pos = self.to_msgbuf_pos(mouse_pos);
        if let Some((msg, msg_top)) = msgbuf.get_line(&rect, msgbuf_pos.y).await {
            msg.handle_mouse_move(Point::new(msgbuf_pos.x, msg_top - msgbuf_pos.y)).await;
        }
        false
    }

    async fn handle_mouse_wheel(&self, wheel_pos: Point) -> bool {
        //t!("handle_mouse_wheel({wheel_pos:?})");

        let rect = self.rect.get();

        let mouse_pos = self.mouse_pos.lock().clone();
        if !rect.contains(mouse_pos) {
            t!("not inside rect");
            return false
        }

        self.start_scroll(wheel_pos.y);
        true
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_pos: Point) -> bool {
        // Ignore multi-touch
        if id != 0 {
            return false
        }

        let rect = self.rect.get();
        //t!("handle_touch({phase:?}, {id},{id},  {touch_pos:?})");

        let touch_y = touch_pos.y;

        if !rect.contains(touch_pos) {
            match phase {
                TouchPhase::Started => *self.touch_info.lock() = None,
                _ => self.end_touch_phase(touch_y),
            }
            return false
        }

        let hold_ms = long_press_timeout() as u64;

        // Simulate mouse events
        match phase {
            TouchPhase::Started => {
                self.touch_is_active.store(true, Ordering::Relaxed);

                *self.touch_info.lock() = Some(TouchInfo::new(self.scroll.get(), touch_y));

                // Arm the long-press timer for text selection.
                let version = self.touch_hold_version.fetch_add(1, Ordering::SeqCst) + 1;
                let me = self.me.clone();
                let ex = self.ex.clone();
                let start_pos = touch_pos;
                ex.spawn(async move {
                    msleep(hold_ms).await;
                    let Some(self_) = me.upgrade() else { return };
                    self_.long_hold_fire(version, start_pos).await;
                })
                .detach();
            }
            TouchPhase::Moved => {
                let (start_scroll, start_y, start_elapsed, do_update, is_select_mode) = {
                    let mut touch_info = self.touch_info.lock();
                    let Some(touch_info) = &mut *touch_info else { return false };

                    touch_info.last_y = touch_y;

                    let start_scroll = touch_info.start_scroll;
                    let start_y = touch_info.start_y;

                    let start_elapsed =
                        touch_info.start_instant.elapsed().as_micros() as f32 / 1000.;
                    let is_select_mode = touch_info.is_select_mode.clone();

                    touch_info.push_sample(touch_y);

                    // Only update screen every 20ms. Avoid wasting cycles.
                    let last_elapsed = touch_info.last_instant.elapsed().as_micros();
                    let do_update = last_elapsed > 20_000;
                    if do_update {
                        touch_info.last_instant = std::time::Instant::now();
                    }

                    (start_scroll, start_y, start_elapsed, do_update, is_select_mode)
                };

                t!("touch phase moved, is_select_mode={is_select_mode:?}");

                // When scrolling if we suddenly grab the screen for more than a brief period
                // of time then stop the scrolling completely.
                if start_elapsed > 200. {
                    t!("Stopping scroll accel");
                    self.speed.store(0., Ordering::Relaxed);
                }

                // Only update every so often to prevent wasting resources.
                if !do_update {
                    return true
                }

                // We are in selection mode so don't scroll the screen until touch phase ends.
                if is_select_mode == Some(true) {
                    self.select_line(touch_y).await;
                    return true
                }

                let dist = touch_y - start_y;
                // No movement so just return
                if dist.abs() < BIG_EPSILON {
                    return true
                }
                let scroll = start_scroll + dist;
                let atom = &mut self.redraw.make_guard(gfxtag!("ChatView::handle_touch_scroll"));
                self.scrollview(scroll, atom).await;
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let (start_y, is_select_mode) = {
                    let touch_info = self.touch_info.lock();
                    let Some(touch_info) = &*touch_info else { return true };
                    (touch_info.start_y, touch_info.is_select_mode)
                };

                // If the timer never fired and movement was minimal, it is a tap.
                if is_select_mode.is_none() && (touch_y - start_y).abs() < BIG_EPSILON {
                    // A tap forwards to the message first (opens a URL / downloads a file).
                    let mut msgbuf = self.msgbuf.lock().await;
                    let msgbuf_pos = self.to_msgbuf_pos(touch_pos);
                    let mut is_handled = false;
                    if let Some((msg, msg_top)) =
                        msgbuf.get_line(&self.rect.get(), msgbuf_pos.y).await
                    {
                        is_handled = msg
                            .handle_touch(
                                TouchPhase::Ended,
                                0,
                                Point::new(msgbuf_pos.x, msg_top - msgbuf_pos.y),
                            )
                            .await
                    }
                    drop(msgbuf);

                    // Not a URL/file tap and selection mode is active: toggle the line.
                    if !is_handled && self.select_active.load(Ordering::Relaxed) {
                        if self.is_line_selected(touch_y).await {
                            self.deselect_line(touch_y).await;
                        } else {
                            self.select_line(touch_y).await;
                        }
                    }
                }

                self.end_touch_phase(touch_y);
            }
        }
        true
    }
}

impl Drop for ChatView {
    fn drop(&mut self) {
        self.renderer.replace_draw_calls(vec![(self.dc_key, Default::default())]);
    }
}

impl std::fmt::Debug for ChatView {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.node.upgrade().unwrap())
    }
}
