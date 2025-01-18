/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use async_lock::Mutex as AsyncMutex;
use async_trait::async_trait;
use atomic_float::AtomicF32;
use chrono::{Local, TimeZone};
use darkfi::system::{msleep, CondVar};
use darkfi_serial::{deserialize, Decodable, Encodable, SerialDecodable, SerialEncodable};
use miniquad::{KeyCode, KeyMods, MouseButton, TouchPhase};
use rand::{rngs::OsRng, Rng};
use sled_overlay::sled;
use std::{
    collections::VecDeque,
    hash::{DefaultHasher, Hash, Hasher},
    io::Cursor,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex as SyncMutex, OnceLock, Weak,
    },
};

mod page;
use page::MessageBuffer;

use crate::{
    gfx::{
        GfxDrawCall, GfxDrawInstruction, GfxDrawMesh, GraphicsEventPublisherPtr, Point, Rectangle,
        RenderApi,
    },
    mesh::{Color, MeshBuilder, COLOR_BLUE, COLOR_GREEN},
    prop::{
        PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyRect, PropertyUint32,
        Role,
    },
    pubsub::Subscription,
    scene::{MethodCallSub, Pimpl, SceneNodeWeak},
    text::{self, Glyph, GlyphPositionIter, TextShaperPtr},
    util::{enumerate, is_whitespace, unixtime},
    ExecutorPtr,
};

use super::{DrawUpdate, OnModify, UIObject};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::chatview", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::chatview", $($arg)*); } }

const EPSILON: f32 = 0.001;
const BIG_EPSILON: f32 = 0.05;

// Disable selecting lines for this release.
const ENABLE_SELECT: bool = false;

fn is_zero(x: f32) -> bool {
    x.abs() < EPSILON
}

/// std::cmp::max() doesn't work on f32
fn max(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ChatMsg {
    pub nick: String,
    pub text: String,
}

pub type Timestamp = u64;

#[derive(Clone, SerialEncodable, SerialDecodable, PartialEq)]
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
            if instant.elapsed().as_millis_f32() <= 40. {
                break
            }
            self.samples.pop_front().unwrap();
        }
    }

    fn first_sample(&self) -> Option<(f32, f32)> {
        self.samples.front().map(|(t, s)| (t.elapsed().as_millis_f32(), *s))
    }
}

pub type ChatViewPtr = Arc<ChatView>;

pub struct ChatView {
    node: SceneNodeWeak,
    tasks: OnceLock<Vec<smol::Task<()>>>,
    render_api: RenderApi,
    text_shaper: TextShaperPtr,

    tree: sled::Tree,
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
    select_hold_time: PropertyFloat32,
    key_scroll_speed: PropertyFloat32,

    /// Scroll accel
    motion_cv: Arc<CondVar>,
    speed: AtomicF32,

    mouse_btn_held: AtomicBool,

    /// Triggers the background loading task to wake up
    bgload_cv: Arc<CondVar>,

    /// We use it when we re-eval rect when its changed via property.
    parent_rect: SyncMutex<Option<Rectangle>>,
}

impl ChatView {
    pub async fn new(
        node: SceneNodeWeak,
        tree: sled::Tree,
        window_scale: PropertyFloat32,
        render_api: RenderApi,
        text_shaper: TextShaperPtr,
        ex: ExecutorPtr,
    ) -> Pimpl {
        t!("ChatView::new()");

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
        let nick_colors = node_ref.get_property("nick_colors").expect("ChatView::nick_colors");
        let hi_bg_color = PropertyColor::wrap(node_ref, Role::Internal, "hi_bg_color").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let debug = PropertyBool::wrap(node_ref, Role::Internal, "debug", 0).unwrap();

        let scroll_start_accel =
            PropertyFloat32::wrap(node_ref, Role::Internal, "scroll_start_accel", 0).unwrap();
        let scroll_resist =
            PropertyFloat32::wrap(node_ref, Role::Internal, "scroll_resist", 0).unwrap();
        let select_hold_time =
            PropertyFloat32::wrap(node_ref, Role::Internal, "select_hold_time", 0).unwrap();
        let key_scroll_speed =
            PropertyFloat32::wrap(node_ref, Role::Internal, "key_scroll_speed", 0).unwrap();

        let motion_cv = Arc::new(CondVar::new());
        let bgload_cv = Arc::new(CondVar::new());

        let self_ = Arc::new(Self {
            node: node.clone(),
            tasks: OnceLock::new(),
            render_api: render_api.clone(),
            text_shaper: text_shaper.clone(),

            tree,
            msgbuf: AsyncMutex::new(MessageBuffer::new(
                node,
                font_size,
                timestamp_font_size,
                timestamp_width,
                line_height,
                message_spacing,
                baseline,
                timestamp_color,
                text_color,
                nick_colors,
                hi_bg_color,
                debug,
                window_scale,
                render_api,
                text_shaper,
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
            select_hold_time,
            key_scroll_speed,

            motion_cv,
            speed: AtomicF32::new(0.),

            mouse_btn_held: AtomicBool::new(false),

            bgload_cv,

            parent_rect: SyncMutex::new(None),
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

    /// Mark line as selected
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
        msgbuf.select_line(y).await;

        self.redraw_cached(&mut msgbuf).await;
    }

    fn end_touch_phase(&self, touch_y: f32) {
        // Now calculate scroll acceleration
        let touch_info = std::mem::replace(&mut *self.touch_info.lock().unwrap(), None);
        let Some(touch_info) = &touch_info else { return };

        self.touch_is_active.store(false, Ordering::Relaxed);

        // No scroll accel with selection mode
        if touch_info.is_select_mode.is_some() {
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

        self.tree.insert(&key, val).unwrap();
        let _ = self.tree.flush_async().await;
        true
    }
    pub async fn handle_insert_line(
        &self,
        timest: Timestamp,
        msg_id: MessageId,
        nick: String,
        text: String,
    ) {
        t!("handle_insert_line({timest}, {msg_id}, {nick}, {text})");

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
            if msgbuf.insert_privmsg(timest, msg_id, nick, text).is_none() {
                // Not visible so no need to redraw
                return
            }
        }

        self.redraw_cached(&mut msgbuf).await;
        self.bgload_cv.notify();
    }
    async fn handle_insert_unconf_line(
        &self,
        timest: Timestamp,
        msg_id: MessageId,
        nick: String,
        text: String,
    ) {
        t!("handle_insert_unconf_line({timest}, {msg_id}, {nick}, {text})");

        // We don't add unconfirmed lines to the db. Maybe we should?

        // Add message to page
        let mut msgbuf = self.msgbuf.lock().await;
        let Some(privmsg) = msgbuf.insert_privmsg(timest, msg_id, nick, text) else { return };
        privmsg.confirmed = false;
        self.redraw_cached(&mut msgbuf).await;
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
            let dist = self.scrollview(scroll).await;

            // We reached the end so just stop
            if is_zero(dist) {
                self.speed.store(0., Ordering::Relaxed);
                return
            }
        }
    }

    async fn handle_bgload(&self) {
        t!("ChatView::handle_bgload()");
        // Do we need to load some more?
        let scroll = self.scroll.get();
        let rect = self.rect.get();
        let top = scroll + rect.h;

        let preload_height = PRELOAD_PAGES as f32 * rect.h;

        let mut msgbuf = self.msgbuf.lock().await;

        let total_height = msgbuf.calc_total_height().await;
        if total_height > top + preload_height {
            // Nothing to do here
            t!("bgloader: buffer is sufficient");
            return
        }

        // Keep loading until this is below 0
        let mut remaining_load_height = top + preload_height - total_height;
        let mut remaining_visible = top - total_height;
        t!("bgloader: remaining px = {remaining_load_height}, remaining_visible={remaining_visible}");

        // Get the current earliest timestamp
        let iter = match msgbuf.oldest_timestamp() {
            Some(oldest_timest) => {
                // iterate from there
                t!("preloading from {oldest_timest}");
                let timest = (oldest_timest - 1).to_be_bytes();
                let mut key = [0u8; 8 + 32];
                key[..8].clone_from_slice(&timest);

                let iter = self.tree.range(..key).rev();
                iter
            }
            None => {
                t!("initial load");
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

            t!("{timest:?} {chatmsg:?}");
            let msg_height = msgbuf.push_privmsg(timest, msg_id, chatmsg.nick, chatmsg.text);

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
        t!("do_redraw = {do_redraw}");
        if do_redraw {
            self.redraw_cached(&mut msgbuf).await;
        }
    }

    async fn scrollview(&self, mut scroll: f32) -> f32 {
        t!("scrollview()");
        let old_scroll = self.scroll.get();

        let rect = self.rect.get();

        let mut msgbuf = self.msgbuf.lock().await;

        // 1/3 of time spent here  ~1.5ms
        if let Some(new_scroll) = self.adjust_scroll(&mut msgbuf, scroll, rect.h).await {
            scroll = new_scroll;
        }

        // 2/3 of time spent here  ~3.3ms
        self.redraw_cached(&mut msgbuf).await;

        self.scroll.set(scroll);
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
        let nonneg_scroll = max(scroll, 0.);

        if scroll < 0. {
            return Some(0.)
        }

        let total_height = msgbuf.calc_total_height().await;
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
    ) -> Vec<GfxDrawInstruction> {
        let scroll = self.scroll.get();

        let total_height = msgbuf.calc_total_height().await;

        // Use this to start from the top
        //let start_pos = if total_height < rect.h { total_height } else { rect.h };
        // We start from the bottom though
        let start_pos = rect.h;

        let mut instrs = vec![];
        //let mut old_drawmesh = vec![];

        let meshes = msgbuf.gen_meshes(rect, scroll).await;

        for (i, (y_pos, mesh)) in enumerate(meshes) {
            // Apply scroll and scissor
            // We use the scissor for scrolling
            // Because we use the scissor, our actual rect is now rect instead of parent_rect
            let off_x = 0.;
            // This calc decides whether scroll is in terms of pages or pixels
            let off_y = (scroll + start_pos - y_pos);
            let pos = Point::from([0., off_y]);

            instrs.push(GfxDrawInstruction::Move(pos));
            instrs.push(GfxDrawInstruction::Draw(mesh));
        }

        instrs
    }

    async fn redraw_cached(&self, msgbuf: &mut MessageBuffer) {
        t!("ChatView::redraw_cached()");
        let timest = unixtime();
        let rect = self.rect.get();

        let mut mesh_instrs = self.get_meshes(msgbuf, &rect).await;

        let mut instrs = vec![GfxDrawInstruction::ApplyView(rect)];
        instrs.append(&mut mesh_instrs);

        let draw_calls =
            vec![(self.dc_key, GfxDrawCall { instrs, dcs: vec![], z_index: self.z_index.get() })];

        self.render_api.replace_draw_calls(timest, draw_calls);
        t!("ChatView::redraw_cached() DONE");
    }

    /// Invalidates cache and redraws everything
    async fn redraw_all(&self) {
        t!("ChatView::redraw_all()");
        let parent_rect = self.parent_rect.lock().unwrap().unwrap().clone();
        self.rect.eval(&parent_rect).expect("unable to eval rect");

        let mut msgbuf = self.msgbuf.lock().await;
        msgbuf.adjust_params();
        msgbuf.clear_meshes();
        self.redraw_cached(&mut msgbuf).await;
        t!("ChatView::redraw_all() DONE");
    }
}

#[async_trait]
impl UIObject for ChatView {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);

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

        let mut on_modify = OnModify::new(ex, self.node.clone(), me.clone());

        async fn reload_view(self_: Arc<ChatView>) {
            self_.scrollview(self_.scroll.get()).await;
        }
        on_modify.when_change(self.scroll.prop(), reload_view);

        async fn redraw(self_: Arc<ChatView>) {
            if !self_.rect.has_cached() {
                return
            }
            self_.redraw_all().await;
        }

        //on_modify.when_change(self.baseline.prop(), redraw);
        //on_modify.when_change(self.font_size.prop(), redraw);
        //on_modify.when_change(self.timestamp_font_size.prop(), redraw);
        //on_modify.when_change(self.timestamp_color.prop(), redraw);
        //on_modify.when_change(self.timestamp_width.prop(), redraw);
        //on_modify.when_change(self.line_height.prop(), redraw);
        //on_modify.when_change(self.message_spacing.prop(), redraw);
        //on_modify.when_change(self.text_color.prop(), redraw);
        //on_modify.when_change(self.nick_colors.clone(), redraw);
        //on_modify.when_change(self.hi_bg_color.prop(), redraw);
        on_modify.when_change(self.rect.prop(), redraw);
        //on_modify.when_change(self.debug.prop(), redraw);

        let mut tasks =
            vec![insert_line_method_task, insert_unconf_line_method_task, motion_task, bgload_task];
        tasks.append(&mut on_modify.tasks);

        self.tasks.set(tasks);
    }

    async fn draw(&self, parent_rect: Rectangle) -> Option<DrawUpdate> {
        t!("ChatView::draw({:?})", self.node.upgrade().unwrap());

        *self.parent_rect.lock().unwrap() = Some(parent_rect.clone());
        self.rect.eval(&parent_rect).ok()?;
        let rect = self.rect.get();

        let mut msgbuf = self.msgbuf.lock().await;
        msgbuf.adjust_window_scale();
        msgbuf.adjust_width(rect.w);
        msgbuf.clear_meshes();

        let mut scroll = self.scroll.get();
        if let Some(scroll) = self.adjust_scroll(&mut msgbuf, scroll, rect.h).await {
            self.scroll.set(scroll);
        }

        // We may need to load more messages since the screen size has changed.
        // Now we have updated all the values so it's safe to wake up here.
        self.bgload_cv.notify();

        let mut mesh_instrs = self.get_meshes(&mut msgbuf, &rect).await;
        drop(msgbuf);

        let mut instrs = vec![GfxDrawInstruction::ApplyView(rect)];
        instrs.append(&mut mesh_instrs);

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                GfxDrawCall { instrs, dcs: vec![], z_index: self.z_index.get() },
            )],
        })
    }

    async fn handle_key_down(&self, key: KeyCode, mods: KeyMods, repeat: bool) -> bool {
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
        if btn != MouseButton::Left {
            return false
        }

        let rect = self.rect.get();
        if !rect.contains(mouse_pos) {
            return false
        }

        if ENABLE_SELECT {
            self.select_line(mouse_pos.y).await;
        }
        self.mouse_btn_held.store(true, Ordering::Relaxed);
        true
    }

    async fn handle_mouse_btn_up(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        t!("handle_mouse_btn_up({btn:?}, {mouse_pos:?})");

        if btn != MouseButton::Left {
            return false
        }

        self.mouse_btn_held.store(false, Ordering::Relaxed);
        false
    }

    async fn handle_mouse_move(&self, mouse_pos: Point) -> bool {
        t!("handle_mouse_move({mouse_pos:?})");

        // We store the mouse pos for use in handle_mouse_wheel()
        *self.mouse_pos.lock().unwrap() = mouse_pos.clone();

        if !self.mouse_btn_held.load(Ordering::Relaxed) {
            return false
        }

        let rect = self.rect.get();
        if !rect.contains(mouse_pos) {
            return false
        }

        if ENABLE_SELECT {
            self.select_line(mouse_pos.y).await;
        }
        false
    }

    async fn handle_mouse_wheel(&self, wheel_pos: Point) -> bool {
        t!("handle_mouse_wheel({wheel_pos:?})");

        let rect = self.rect.get();

        let mouse_pos = self.mouse_pos.lock().unwrap().clone();
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
        t!("handle_touch({phase:?}, {id},{id},  {touch_pos:?})");

        let touch_y = touch_pos.y;

        if !rect.contains(touch_pos) {
            match phase {
                TouchPhase::Started => *self.touch_info.lock().unwrap() = None,
                _ => self.end_touch_phase(touch_y),
            }
            return false
        }

        let select_hold_time = self.select_hold_time.get();

        // Simulate mouse events
        match phase {
            TouchPhase::Started => {
                self.touch_is_active.store(true, Ordering::Relaxed);

                let mut touch_info = self.touch_info.lock().unwrap();
                *touch_info = Some(TouchInfo::new(self.scroll.get(), touch_y));
            }
            TouchPhase::Moved => {
                let (start_scroll, start_y, start_elapsed, do_update, is_select_mode) = {
                    let mut touch_info = self.touch_info.lock().unwrap();
                    let Some(touch_info) = &mut *touch_info else { return false };

                    touch_info.last_y = touch_y;

                    let start_scroll = touch_info.start_scroll;
                    let start_y = touch_info.start_y;

                    let start_elapsed = touch_info.start_instant.elapsed().as_millis_f32();
                    if start_elapsed > select_hold_time && touch_info.is_select_mode.is_none() {
                        // Did we move?
                        if (touch_y - start_y).abs() < BIG_EPSILON {
                            touch_info.is_select_mode = Some(true);
                        } else {
                            touch_info.is_select_mode = Some(false);
                        }
                    }
                    let is_select_mode = touch_info.is_select_mode.clone();

                    touch_info.push_sample(touch_y);

                    // Only update screen every 20ms. Avoid wasting cycles.
                    let last_elapsed = touch_info.last_instant.elapsed().as_millis_f32();
                    let do_update = last_elapsed > 20.;
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
                if let Some(is_select_mode) = is_select_mode &&
                    is_select_mode
                {
                    if ENABLE_SELECT {
                        self.select_line(touch_y).await;
                    }
                    return true
                }

                let dist = touch_y - start_y;
                // No movement so just return
                if dist.abs() < BIG_EPSILON {
                    return true
                }
                let scroll = start_scroll + dist;
                // Redraws the screen from the cache
                self.scrollview(scroll).await;
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.end_touch_phase(touch_y);
            }
        }
        true
    }
}

impl Drop for ChatView {
    fn drop(&mut self) {
        self.render_api.replace_draw_calls(unixtime(), vec![(self.dc_key, Default::default())]);
    }
}
