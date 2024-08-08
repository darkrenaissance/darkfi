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
use atomic_float::AtomicF32;
use chrono::{Local, TimeZone};
use darkfi::system::{msleep, CondVar};
use darkfi_serial::{
    async_trait, deserialize, Decodable, Encodable, FutAsyncWriteExt, ReadExt, SerialDecodable,
    SerialEncodable, VarInt,
};
use miniquad::{KeyCode, KeyMods, TouchPhase};
use rand::{rngs::OsRng, Rng};
use std::{
    collections::BTreeMap,
    hash::{DefaultHasher, Hash, Hasher},
    io::Cursor,
    sync::{atomic::Ordering, Arc, Mutex as SyncMutex, Weak},
};

use crate::{
    error::Result,
    gfx2::{
        DrawCall, DrawInstruction, DrawMesh, GraphicsEventPublisherPtr, Point, Rectangle,
        RenderApi, RenderApiPtr, Vertex,
    },
    mesh::{Color, MeshBuilder, MeshInfo, COLOR_BLUE, COLOR_GREEN, COLOR_GREY, COLOR_WHITE},
    prop::{
        PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyStr, PropertyUint32,
        Role,
    },
    pubsub::Subscription,
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
    text2::{self, Glyph, GlyphPositionIter, SpritePtr, TextShaper, TextShaperPtr},
    util::zip3,
    ExecutorPtr,
};

use super::{eval_rect, get_parent_rect, read_rect, DrawUpdate, OnModify, Stoppable};

const DEBUG_RENDER: bool = false;

const EPSILON: f32 = 0.001;
const BIG_EPSILON: f32 = 0.05;

fn is_whitespace(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
}

fn is_zero(x: f32) -> bool {
    x.abs() < EPSILON
}

// Replace vec item with N items
fn replace_vec_item<T>(vec: &mut Vec<T>, idx: usize, mut items: Vec<T>) {
    assert!(idx < vec.len());
    if items.len() == 1 {
        let item = items.remove(0);
        std::mem::replace(&mut vec[idx], item);
        return
    }

    let mut drain_iter = vec.drain(idx..);
    // Drop the item at idx which will be replaced
    drain_iter.next().unwrap();
    let mut tail: Vec<_> = drain_iter.collect();

    vec.append(&mut items);
    vec.append(&mut tail);
}

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ChatMsg {
    pub nick: String,
    pub text: String,
}

type Timestamp = u64;
type MessageId = [u8; 32];

#[derive(Clone)]
struct Message {
    timest: Timestamp,
    id: MessageId,
    chatmsg: ChatMsg,
    glyphs: Vec<Glyph>,
}

const PAGE_SIZE: usize = 10;
const PRELOAD_PAGES: usize = 10;

#[derive(Clone)]
struct Page {
    msgs: Vec<Message>,
    atlas: text2::RenderedAtlas,
}

#[derive(Clone)]
struct PageMeshInfo {
    px_height: f32,
    mesh: DrawMesh,
}

type Page2Ptr = Arc<Page2>;

struct Page2 {
    msgs: Vec<Message>,
    atlas: SyncMutex<text2::RenderedAtlas>,
    // One draw call per page.
    // Resizing the canvas means we recalc wrapping and the mesh changes
    mesh_inf: SyncMutex<Option<PageMeshInfo>>,
}

impl Page2 {
    async fn new(msgs: Vec<Message>, render_api: &RenderApi) -> Arc<Self> {
        let mut atlas = text2::Atlas::new(render_api);
        for msg in &msgs {
            atlas.push(&msg.glyphs);
        }
        let Ok(atlas) = atlas.make().await else {
            // what else should I do here?
            panic!("unable to make atlas!");
        };

        Arc::new(Self { msgs, atlas: SyncMutex::new(atlas), mesh_inf: SyncMutex::new(None) })
    }

    /// Regenerates the mesh, returning the old mesh which should be freed
    async fn regen_mesh(
        &self,
        clip: &Rectangle,
        render_api: &RenderApi,
        font_size: f32,
        line_height: f32,
        baseline: f32,
        nick_colors: &[Color],
        timestamp_color: Color,
        text_color: Color,
        debug_render: bool,
    ) -> (PageMeshInfo, Option<DrawMesh>) {
        let mut wrapped_line_idx = 0;

        let mut mesh = MeshBuilder::new();

        let atlas = self.atlas.lock().unwrap().clone();
        for msg in &self.msgs {
            let glyphs = &msg.glyphs;

            let nick_color = select_nick_color(&msg.chatmsg.nick, nick_colors);

            // Keep track of the 'section'
            // Section 0 is the timestamp
            // Section 1 is the nickname (colorized)
            // Finally is just the message itself
            let mut section = 2;

            let mut lines = text2::wrap(clip.w, font_size, glyphs);
            // We are drawing bottom up but line wrap gives us lines in normal order
            lines.reverse();
            let last_idx = lines.len() - 1;
            for (i, line) in lines.into_iter().enumerate() {
                let off_y = (wrapped_line_idx + 1) as f32 * line_height;

                if i == last_idx {
                    section = 0;
                }

                // debug draw baseline
                if debug_render {
                    let y = baseline - off_y;
                    mesh.draw_filled_box(
                        &Rectangle { x: 0., y: y - 1., w: clip.w, h: 1. },
                        COLOR_BLUE,
                    );
                }

                // Render line
                let mut glyph_pos_iter = GlyphPositionIter::new(font_size, &line, baseline);
                for (mut glyph_rect, glyph) in glyph_pos_iter.zip(line.iter()) {
                    let uv_rect = atlas.fetch_uv(glyph.glyph_id).expect("missing glyph UV rect");
                    glyph_rect.y -= off_y;

                    let color = match section {
                        0 => timestamp_color,
                        1 => nick_color,
                        _ => text_color,
                    };

                    //if debug_render {
                    //    mesh.draw_outline(&glyph_rect, COLOR_BLUE, 2.);
                    //}

                    mesh.draw_box(&glyph_rect, color, uv_rect);

                    if section < 2 && is_whitespace(&glyph.substr) {
                        section += 1;
                    }
                }
                wrapped_line_idx += 1;
            }
        }

        let px_height = wrapped_line_idx as f32 * line_height;

        if debug_render {
            mesh.draw_outline(
                &Rectangle { x: 0., y: 0., w: clip.w, h: -px_height },
                COLOR_GREEN,
                1.,
            );
        }

        let mesh = mesh.alloc(render_api).await.unwrap();
        let mesh = mesh.draw_with_texture(atlas.texture_id);

        let mesh_inf = PageMeshInfo { px_height, mesh };

        let old = std::mem::replace(&mut *self.mesh_inf.lock().unwrap(), Some(mesh_inf.clone()));
        let old = old.map(|v| v.mesh);

        (mesh_inf, old)
    }
}

fn select_nick_color(nick: &str, nick_colors: &[Color]) -> Color {
    let mut hasher = DefaultHasher::new();
    nick.hash(&mut hasher);
    let i = hasher.finish() as usize;
    let color = nick_colors[i % nick_colors.len()];
    color
}

#[derive(Clone)]
struct TouchInfo {
    start_scroll: f32,
    start_y: f32,
    start_instant: std::time::Instant,
    last_y: f32,
}

impl TouchInfo {
    fn new() -> Self {
        Self { start_scroll: 0., start_y: 0., start_instant: std::time::Instant::now(), last_y: 0. }
    }
}

pub type ChatViewPtr = Arc<ChatView>;

pub struct ChatView {
    node_id: SceneNodeId,
    tasks: Vec<smol::Task<()>>,
    sg: SceneGraphPtr2,
    render_api: RenderApiPtr,
    text_shaper: TextShaperPtr,
    tree: sled::Tree,

    pages: SyncMutex<Vec<Page>>,
    pages2: AsyncMutex<Vec<Page2Ptr>>,
    drawcalls: SyncMutex<Vec<DrawMesh>>,
    dc_key: u64,

    /// Used for detecting when scrolling view
    mouse_pos: SyncMutex<Point>,
    /// Touch scrolling
    touch_info: SyncMutex<TouchInfo>,

    rect: PropertyPtr,
    scroll: PropertyFloat32,
    font_size: PropertyFloat32,
    line_height: PropertyFloat32,
    baseline: PropertyFloat32,
    timestamp_color: PropertyColor,
    text_color: PropertyColor,
    nick_colors: PropertyPtr,
    z_index: PropertyUint32,
    debug: PropertyBool,

    mouse_scroll_start_accel: PropertyFloat32,
    mouse_scroll_decel: PropertyFloat32,
    mouse_scroll_resist: PropertyFloat32,

    // Scroll accel
    motion_cv: Arc<CondVar>,
    accel: AtomicF32,
    speed: AtomicF32,
}

impl ChatView {
    pub async fn new(
        ex: ExecutorPtr,
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        render_api: RenderApiPtr,
        event_pub: GraphicsEventPublisherPtr,
        text_shaper: TextShaperPtr,
        tree: sled::Tree,
        recvr: async_channel::Receiver<Vec<u8>>,
    ) -> Pimpl {
        debug!(target: "ui::chatview", "ChatView::new()");
        let scene_graph = sg.lock().await;
        let node = scene_graph.get_node(node_id).unwrap();
        let node_name = node.name.clone();

        let rect = node.get_property("rect").expect("ChatView::rect");
        let scroll = PropertyFloat32::wrap(node, Role::Internal, "scroll", 0).unwrap();
        let font_size = PropertyFloat32::wrap(node, Role::Internal, "font_size", 0).unwrap();
        let line_height = PropertyFloat32::wrap(node, Role::Internal, "line_height", 0).unwrap();
        let baseline = PropertyFloat32::wrap(node, Role::Internal, "baseline", 0).unwrap();
        let timestamp_color = PropertyColor::wrap(node, Role::Internal, "timestamp_color").unwrap();
        let text_color = PropertyColor::wrap(node, Role::Internal, "text_color").unwrap();
        let nick_colors = node.get_property("nick_colors").expect("ChatView::nick_colors");
        let z_index = PropertyUint32::wrap(node, Role::Internal, "z_index", 0).unwrap();
        let debug = PropertyBool::wrap(node, Role::Internal, "debug", 0).unwrap();

        let mouse_scroll_start_accel =
            PropertyFloat32::wrap(node, Role::Internal, "mouse_scroll_start_accel", 0).unwrap();
        let mouse_scroll_decel =
            PropertyFloat32::wrap(node, Role::Internal, "mouse_scroll_decel", 0).unwrap();
        let mouse_scroll_resist =
            PropertyFloat32::wrap(node, Role::Internal, "mouse_scroll_resist", 0).unwrap();
        drop(scene_graph);

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            let ev_sub = event_pub.subscribe_mouse_wheel();
            let me2 = me.clone();
            let mouse_wheel_task =
                ex.spawn(async move { while Self::process_mouse_wheel(&me2, &ev_sub).await {} });

            let ev_sub = event_pub.subscribe_mouse_move();
            let me2 = me.clone();
            let mouse_move_task =
                ex.spawn(async move { while Self::process_mouse_move(&me2, &ev_sub).await {} });

            let ev_sub = event_pub.subscribe_touch();
            let me2 = me.clone();
            let touch_task =
                ex.spawn(async move { while Self::process_touch(&me2, &ev_sub).await {} });

            let ev_sub = event_pub.subscribe_key_down();
            let me2 = me.clone();
            let key_down_task =
                ex.spawn(async move { while Self::process_key_down(&me2, &ev_sub).await {} });

            let me2 = me.clone();
            let insert_line_method_task =
                ex.spawn(
                    async move { while Self::process_insert_line_method(&me2, &recvr).await {} },
                );

            let me2 = me.clone();
            let motion_cv = Arc::new(CondVar::new());
            let cv = motion_cv.clone();
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

            let mut on_modify = OnModify::new(ex, node_name, node_id, me.clone());

            async fn reload_view(self_: Arc<ChatView>) {
                self_.scrollview(self_.scroll.get()).await;
            }
            on_modify.when_change(scroll.prop(), reload_view);

            async fn redraw(self_: Arc<ChatView>) {
                self_.redraw().await;
            }
            on_modify.when_change(rect.clone(), redraw);
            on_modify.when_change(debug.prop(), redraw);

            let mut tasks = vec![
                mouse_wheel_task,
                mouse_move_task,
                touch_task,
                key_down_task,
                insert_line_method_task,
                motion_task,
            ];
            tasks.append(&mut on_modify.tasks);

            Self {
                node_id,
                tasks,
                sg,
                render_api,
                text_shaper,
                tree,

                pages: SyncMutex::new(Vec::new()),
                pages2: AsyncMutex::new(Vec::new()),
                drawcalls: SyncMutex::new(Vec::new()),
                dc_key: OsRng.gen(),

                mouse_pos: SyncMutex::new(Point::from([0., 0.])),
                touch_info: SyncMutex::new(TouchInfo::new()),

                rect,
                scroll,
                font_size,
                line_height,
                baseline,
                timestamp_color,
                text_color,
                nick_colors,
                z_index,
                debug,

                mouse_scroll_start_accel,
                mouse_scroll_decel,
                mouse_scroll_resist,

                motion_cv,
                accel: AtomicF32::new(0.),
                speed: AtomicF32::new(0.),
            }
        });

        let timer = std::time::Instant::now();
        self_.populate().await;
        debug!(target: "ui::chatview", "populate() took {:?}", timer.elapsed());

        Pimpl::ChatView(self_)
    }

    async fn process_mouse_wheel(me: &Weak<Self>, ev_sub: &Subscription<(f32, f32)>) -> bool {
        let Ok((wheel_x, wheel_y)) = ev_sub.receive().await else {
            debug!(target: "ui::chatview", "Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before mouse_wheel_task was stopped!");
        };

        self_.handle_mouse_wheel(wheel_x, wheel_y).await;
        true
    }

    async fn process_mouse_move(me: &Weak<Self>, ev_sub: &Subscription<(f32, f32)>) -> bool {
        let Ok((mouse_x, mouse_y)) = ev_sub.receive().await else {
            debug!(target: "ui::chatview", "Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before mouse_move_task was stopped!");
        };

        self_.handle_mouse_move(mouse_x, mouse_y).await;
        true
    }

    async fn process_touch(
        me: &Weak<Self>,
        ev_sub: &Subscription<(TouchPhase, u64, f32, f32)>,
    ) -> bool {
        let Ok((phase, id, touch_x, touch_y)) = ev_sub.receive().await else {
            debug!(target: "ui::chatview", "Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before touch_task was stopped!");
        };

        self_.handle_touch(phase, id, touch_x, touch_y).await;
        true
    }

    async fn process_key_down(
        me: &Weak<Self>,
        ev_sub: &Subscription<(KeyCode, KeyMods, bool)>,
    ) -> bool {
        let Ok((key, mods, repeat)) = ev_sub.receive().await else {
            debug!(target: "ui::editbox", "Event relayer closed");
            return false
        };

        if repeat {
            return true
        }

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before char_task was stopped!");
        };

        match key {
            KeyCode::PageUp => {
                let scroll = self_.scroll.get() + 200.;
                self_.scrollview(scroll).await;
            }
            KeyCode::PageDown => {
                let scroll = self_.scroll.get() - 200.;
                self_.scrollview(scroll).await;
            }
            _ => {}
        }

        true
    }

    async fn process_insert_line_method(
        me: &Weak<Self>,
        recvr: &async_channel::Receiver<Vec<u8>>,
    ) -> bool {
        let Ok(data) = recvr.recv().await else {
            debug!(target: "ui::chatview", "Event relayer closed");
            return false
        };

        fn decode_data(data: &[u8]) -> std::io::Result<(Timestamp, MessageId, String, String)> {
            let mut cur = Cursor::new(&data);
            let timestamp = Timestamp::decode(&mut cur)?;
            let message_id = MessageId::decode(&mut cur)?;
            let nick = String::decode(&mut cur)?;
            let text = String::decode(&mut cur)?;
            Ok((timestamp, message_id, nick, text))
        }

        let Ok((timestamp, message_id, nick, text)) = decode_data(&data) else {
            error!(target: "ui::chatview", "insert_line() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before touch_task was stopped!");
        };

        self_.handle_insert_line(timestamp, message_id, nick, text).await;
        true
    }

    async fn handle_mouse_wheel(&self, wheel_x: f32, wheel_y: f32) {
        debug!(target: "ui::chatview", "handle_mouse_wheel({wheel_x}, {wheel_y})");

        let Some(rect) = self.get_cached_world_rect().await else { return };

        let mouse_pos = self.mouse_pos.lock().unwrap().clone();
        if !rect.contains(&mouse_pos) {
            //debug!(target: "ui::chatview", "not inside rect");
            return
        }

        //debug!(target: "ui::chatview", "inside rect");
        //let scroll = self.scroll.get() + wheel_y * 50.;
        //self.scrollview(scroll).await;

        self.accel.fetch_add(wheel_y * self.mouse_scroll_start_accel.get(), Ordering::Relaxed);
        self.motion_cv.notify();
    }

    async fn handle_mouse_move(&self, mouse_x: f32, mouse_y: f32) {
        //debug!(target: "ui::chatview", "handle_mouse_move({mouse_x}, {mouse_y})");
        let mut mouse_pos = self.mouse_pos.lock().unwrap();
        mouse_pos.x = mouse_x;
        mouse_pos.y = mouse_y;
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_x: f32, touch_y: f32) {
        // Ignore multi-touch
        if id != 0 {
            return
        }
        // Simulate mouse events
        match phase {
            TouchPhase::Started => {
                let mut touch_info = self.touch_info.lock().unwrap();
                touch_info.start_scroll = self.scroll.get();
                touch_info.start_y = touch_y;
                touch_info.start_instant = std::time::Instant::now();
                touch_info.last_y = touch_y;
            }
            TouchPhase::Moved => {
                let (start_scroll, start_y) = {
                    let mut touch_info = self.touch_info.lock().unwrap();
                    touch_info.last_y = touch_y;
                    (touch_info.start_scroll, touch_info.start_y)
                };

                let dist = touch_y - start_y;
                // TODO the line selected should be fixed and move exactly that distance
                // No use of multipliers
                // TODO we are maybe doing too many updates so make a widget to 'slow down'
                // how often we move to fixed intervals.
                // draw a poly shape and eval each line segment.
                let scroll = start_scroll + dist;
                self.scrollview(scroll).await;
            }
            TouchPhase::Ended => {
                // Now calculate scroll acceleration
                let touch_info = self.touch_info.lock().unwrap().clone();

                let time = touch_info.start_instant.elapsed().as_millis_f32();
                let dist = touch_y - touch_info.start_y;

                let accel = self.mouse_scroll_start_accel.get() * dist / time;
                self.accel.fetch_add(accel, Ordering::Relaxed);
                self.motion_cv.notify();
            }
            TouchPhase::Cancelled => {}
        }
    }

    fn add_line_to_db(&self,
        timest: Timestamp,
        message_id: &MessageId,
        nick: &str,
        text: &str,
    ) -> bool {
        let timest = timest.to_be_bytes();
        assert_eq!(timest.len(), 8);
        let mut key = [0u8; 8 + 32];
        key[..8].clone_from_slice(&timest);
        key[8..].clone_from_slice(message_id);

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
        true
    }
    async fn handle_insert_line(
        &self,
        timest: Timestamp,
        message_id: MessageId,
        nick: String,
        text: String,
    ) {
        debug!(target: "ui::chatview", "handle_insert_line({timest}, {message_id:?}, {nick}, {text})");

        if !self.add_line_to_db(timest, &message_id, &nick, &text) {
            // Already exists so bail
            debug!(target: "ui::chatview", "duplicate msg so bailing");
            return
        }

        let chatmsg = ChatMsg { nick, text };

        let dt = Local.timestamp_opt(timest as i64, 0).unwrap();
        let timestr = dt.format("%H:%M").to_string();

        let text = format!("{} {} {}", timestr, chatmsg.nick, chatmsg.text);
        let glyphs = self.text_shaper.shape(text, self.font_size.get()).await;

        // Now add message to page

        let mut pages = self.pages2.lock().await;
        let mut idx = None;
        for (i, page) in pages.iter_mut().enumerate() {
            let first_timest = page.msgs.last().unwrap().timest;
            let last_timest = page.msgs.first().unwrap().timest;

            //debug!(target: "ui::chatview", "page {i} [{first_timest}, {last_timest}]");
            if first_timest <= timest && timest <= last_timest {
                //debug!(target: "ui::chatview", "found page {i} [{first_timest}, {last_timest}]");
                idx = Some(i);
                break
            }
        }
        let idx = match idx {
            Some(idx) => idx,
            None => {
                //debug!(target: "ui::chatview", "no page found");
                0
            }
        };

        // Maybe we can write this code below better
        if pages.is_empty() {
            let msgs = vec![Message { timest, id: message_id, chatmsg, glyphs }];
            let page = Page2::new(msgs, &self.render_api).await;
            pages.push(page);
            drop(pages);

            let mut scroll = self.scroll.get();
            self.scrollview(scroll).await;
            return;
        }

        let page = &mut pages[idx];
        let mut msgs = page.msgs.clone();
        msgs.push(Message { timest, id: message_id, chatmsg, glyphs });
        msgs.sort_unstable_by_key(|msg| msg.timest);
        msgs.reverse();

        let chunk_size = if msgs.len() > PAGE_SIZE {
            // Round up so we don't get a weird page with a single item
            msgs.len() / 2 + 1
        } else {
            PAGE_SIZE
        };

        // Replace single page with N pages each with chunk_size messages
        let mut new_pages = vec![];
        for page_msgs in msgs.chunks(chunk_size).map(|m| m.to_vec()) {
            debug!(target: "ui::chatview", "PAGE ==========================");
            for msg in &page_msgs {
                debug!(target: "ui::chatview", "{} {:?}", msg.timest, msg.chatmsg);
            }
            debug!(target: "ui::chatview", "===============================");

            let new_page = Page2::new(page_msgs, &self.render_api).await;
            new_pages.push(new_page);
        }

        replace_vec_item(&mut pages, idx, new_pages);

        drop(pages);

        // This will refresh the view, so we just use this
        let mut scroll = self.scroll.get();
        self.scrollview(scroll).await;
    }

    async fn handle_movement(&self) {
        loop {
            msleep(20).await;
            let mut accel = self.accel.load(Ordering::Relaxed);
            let mut speed = self.speed.fetch_add(accel, Ordering::Relaxed) + accel;
            accel *= self.mouse_scroll_decel.get();
            if accel.abs() < 0.05 {
                accel = 0.;
            }
            self.accel.store(accel, Ordering::Relaxed);

            // Apply constant decel to speed
            if is_zero(accel) {
                speed *= self.mouse_scroll_resist.get();
                if speed.abs() < BIG_EPSILON {
                    speed = 0.;
                }
                self.speed.store(speed, Ordering::Relaxed);
            }

            // Finished
            if is_zero(accel) && is_zero(speed) {
                return
            }

            if is_zero(speed) {
                self.accel.store(0., Ordering::Relaxed);
                self.speed.store(0., Ordering::Relaxed);
                return
            }

            let scroll = self.scroll.get() + speed;
            let dist = self.scrollview(scroll).await;

            if is_zero(dist) {
                self.accel.store(0., Ordering::Relaxed);
                self.speed.store(0., Ordering::Relaxed);
                return
            }
        }
    }

    /// Descent = line height - baseline
    fn descent(&self) -> f32 {
        self.line_height.get() - self.baseline.get()
    }

    fn clamp_scroll(scroll: &mut f32, total_height: f32, rect_h: f32) -> bool {
        assert!(*scroll >= 0.);

        // For when we resize the window and scroll is no longer valid
        let max_allowed_scroll = if total_height > rect_h { total_height - rect_h } else { 0. };
        debug!(
            target: "ui::chatview",
            "max_allowed_scroll = {max_allowed_scroll} = total_height={total_height} - rect.h={}",
            rect_h
        );

        if *scroll > max_allowed_scroll {
            *scroll = max_allowed_scroll;
            assert!(*scroll >= 0.);
            return true
        }

        // Unchanged
        false
    }

    /// Beware of this method. Here be dragons.
    /// Possibly racy so we limit it just to mouse stuff (for now).
    fn cached_rect(&self) -> Option<Rectangle> {
        let Ok(rect) = read_rect(self.rect.clone()) else {
            error!(target: "ui::chatview", "cached_rect is None");
            return None
        };
        Some(rect)
    }
    async fn get_parent_rect(&self) -> Option<Rectangle> {
        let sg = self.sg.lock().await;
        let node = sg.get_node(self.node_id).unwrap();
        let Some(parent_rect) = get_parent_rect(&sg, node) else {
            return None;
        };
        drop(sg);
        Some(parent_rect)
    }
    async fn get_cached_world_rect(&self) -> Option<Rectangle> {
        // NBD if it's slightly wrong
        let mut rect = self.cached_rect()?;

        // If layers can be nested and we use offsets for (x, y)
        // then this will be incorrect for nested layers.
        // For now we don't allow nesting of layers.
        let parent_rect = self.get_parent_rect().await?;

        // Offset rect which is now in world coords
        rect.x += parent_rect.x;
        rect.y += parent_rect.y;

        Some(rect)
    }

    async fn populate(&self) {
        debug!(target: "ui::chatview", "populating pages");
        let iter = self.tree.iter().rev();
        self.load_n_pages(iter, PRELOAD_PAGES).await;
    }

    /// Load extra pages
    async fn preload_pages(&self) -> usize {
        // Get last page
        let last_page = self.pages2.lock().await.last().unwrap().clone();
        // get the current earliest timestamp
        let last_timest = last_page.msgs.last().unwrap().timest;

        // iterate from there
        let key = last_timest.to_be_bytes();
        debug!(target: "ui::chatview", "preloading from {key:?}");
        let iter = self.tree.range(..key).rev();

        self.load_n_pages(iter, PRELOAD_PAGES).await
    }

    async fn load_n_pages<I: Iterator<Item = sled::Result<(sled::IVec, sled::IVec)>>>(
        &self,
        iter: I,
        n: usize,
    ) -> usize {
        let mut pages_len = 0;
        let mut msgs = vec![];

        for entry in iter {
            let Ok((k, v)) = entry else { break };
            assert_eq!(k.len(), 8 + 32);
            let timest_bytes: [u8; 8] = k[..8].try_into().unwrap();
            let message_id: MessageId = k[8..].try_into().unwrap();
            let timest = Timestamp::from_be_bytes(timest_bytes);
            let chatmsg: ChatMsg = deserialize(&v).unwrap();
            debug!(target: "ui::chatview", "{timest:?} {chatmsg:?}");

            let dt = Local.timestamp_opt(timest as i64, 0).unwrap();
            let timestr = dt.format("%H:%M").to_string();

            let text = format!("{} {} {}", timestr, chatmsg.nick, chatmsg.text);
            let glyphs = self.text_shaper.shape(text, self.font_size.get()).await;

            msgs.push(Message { timest, id: message_id, chatmsg, glyphs });

            if msgs.len() >= PAGE_SIZE {
                debug!(target: "ui::chatview", "added new page. page_len={pages_len}");
                let msgs = std::mem::take(&mut msgs);
                let page = Page2::new(msgs, &self.render_api).await;

                self.pages2.lock().await.push(page);
                pages_len += 1;

                if pages_len >= n {
                    break
                }
            }
        }

        // Any remaining messages added to a short page
        if !msgs.is_empty() {
            debug!(target: "ui::chatview", "added final page. page_len={pages_len}");
            let page = Page2::new(msgs, &self.render_api).await;

            self.pages2.lock().await.push(page);
            pages_len += 1;
        }

        debug!(target: "ui::chatview", "populated {} pages", pages_len);

        pages_len
    }

    async fn get_total_height(&self, rect: &Rectangle, pages: &Vec<Page2Ptr>) -> f32 {
        let font_size = self.font_size.get();
        let line_height = self.line_height.get();
        let baseline = self.baseline.get();
        let debug_render = self.debug.get();

        let timest_color = self.timestamp_color.get();
        let text_color = self.text_color.get();
        let nick_colors = self.read_nick_colors();

        // Nudge the bottom line up slightly, otherwise chars like p will cross the bottom.
        let mut current_height = self.descent();

        for page in pages {
            let mesh_inf = page.mesh_inf.lock().unwrap().clone();
            let mesh_inf = match mesh_inf {
                Some(mesh_inf) => mesh_inf,
                None => {
                    let (mesh_inf, old_drawmesh) = page
                        .regen_mesh(
                            &rect,
                            &self.render_api,
                            font_size,
                            line_height,
                            baseline,
                            &nick_colors,
                            timest_color.clone(),
                            text_color.clone(),
                            debug_render,
                        )
                        .await;
                    assert!(old_drawmesh.is_none());
                    mesh_inf
                }
            };

            current_height += mesh_inf.px_height;
        }

        current_height
    }

    async fn draw_cached(&self, mut rect: Rectangle, scroll: &mut f32) -> Vec<DrawInstruction> {
        let mut instrs = vec![];

        // When scrolling it can go negative so clamp it here
        if *scroll < 0. {
            *scroll = 0.;
        }

        // Make sure we have enough pages loaded.
        // If there's no more to load then adjust the scroll.
        let mut pages = self.pages2.lock().await.clone();
        let mut total_height = self.get_total_height(&rect, &pages).await;
        while total_height < *scroll + rect.h {
            debug!(target: "ui::chatview", "draw_cached() loading more pages");

            // No more pages available to load
            let n_loaded_pages = self.preload_pages().await;

            // We need this value after so first update it
            total_height = self.get_total_height(&rect, &pages).await;

            if n_loaded_pages == 0 {
                break
            }

            pages = self.pages2.lock().await.clone();
        }

        // If lines aren't enough to fill the available buffer then start from the top
        let start_pos = if total_height < rect.h { total_height } else { rect.h };

        Self::clamp_scroll(scroll, total_height, rect.h);

        let mut current_height = 0.;
        for page in pages {
            if current_height > *scroll + rect.h {
                break
            }

            let mesh_inf = page.mesh_inf.lock().unwrap().clone();
            let mesh_inf = mesh_inf.expect("preload above should regen mesh");

            // Apply scroll and scissor
            // We use the scissor for scrolling
            // Because we use the scissor, our actual rect is now rect instead of parent_rect
            let off_x = 0.;
            // This calc decides whether scroll is in terms of pages or pixels
            let off_y = (*scroll + start_pos - current_height) / rect.h;
            let scale_x = 1. / rect.w;
            let scale_y = 1. / rect.h;
            let model = glam::Mat4::from_translation(glam::Vec3::new(off_x, off_y, 0.)) *
                glam::Mat4::from_scale(glam::Vec3::new(scale_x, scale_y, 1.));

            instrs.push(DrawInstruction::ApplyMatrix(model));
            instrs.push(DrawInstruction::Draw(mesh_inf.mesh));

            current_height += mesh_inf.px_height;
        }

        instrs
    }

    /// Basically a version of redraw() where regen_mesh() is never called.
    /// Instead we use the cached version.
    async fn scrollview(&self, mut scroll: f32) -> f32 {
        debug!(target: "ui::chatview", "scrollview()");
        let old_scroll = self.scroll.get();

        let sg = self.sg.lock().await;
        let node = sg.get_node(self.node_id).unwrap();

        let Some(parent_rect) = get_parent_rect(&sg, node) else {
            return 0.;
        };

        if let Err(err) = eval_rect(self.rect.clone(), &parent_rect) {
            panic!("Node {:?} bad rect property: {}", node, err);
        }

        let Ok(mut rect) = read_rect(self.rect.clone()) else {
            panic!("Node {:?} bad rect property", node);
        };

        let mut mesh_instrs = self.draw_cached(rect.clone(), &mut scroll).await;
        let mut instrs = vec![DrawInstruction::ApplyViewport(rect)];
        instrs.append(&mut mesh_instrs);

        let draw_calls =
            vec![(self.dc_key, DrawCall { instrs, dcs: vec![], z_index: self.z_index.get() })];

        self.render_api.replace_draw_calls(draw_calls).await;

        self.scroll.set(scroll);
        scroll - old_scroll
    }

    async fn redraw(&self) {
        debug!(target: "ui::chatview", "redraw()");
        let sg = self.sg.lock().await;
        let node = sg.get_node(self.node_id).unwrap();

        let Some(parent_rect) = get_parent_rect(&sg, node) else {
            return;
        };

        let Some(draw_update) = self.draw(&sg, &parent_rect).await else {
            error!(target: "ui::chatview", "ChatView {:?} failed to draw", node);
            return;
        };
        self.render_api.replace_draw_calls(draw_update.draw_calls).await;
        debug!(target: "ui::chatview", "replace draw calls done");
        for buffer_id in draw_update.freed_buffers {
            self.render_api.delete_buffer(buffer_id);
        }
        for texture_id in draw_update.freed_textures {
            self.render_api.delete_texture(texture_id);
        }
    }

    async fn regen_mesh(&self, mut rect: Rectangle) -> (Vec<DrawInstruction>, Vec<DrawMesh>) {
        let font_size = self.font_size.get();
        let line_height = self.line_height.get();
        let baseline = self.baseline.get();
        let debug_render = self.debug.get();

        let mut instrs = vec![];
        let mut old_drawmesh = vec![];

        let timest_color = self.timestamp_color.get();
        let text_color = self.text_color.get();
        let nick_colors = self.read_nick_colors();

        let pages = self.pages2.lock().await.clone();

        let mut mesh_infs = vec![];
        // First pass is to measure the height and generate the meshes
        let mut current_height = self.descent();
        for page in pages {
            // We should be able to count lines and perform wrapping without having to
            // generate the mesh and alloc buffers.
            // We need to separate both these ops.
            let (mesh_inf, old) = page
                .regen_mesh(
                    &rect,
                    &self.render_api,
                    font_size,
                    line_height,
                    baseline,
                    &nick_colors,
                    timest_color.clone(),
                    text_color.clone(),
                    debug_render,
                )
                .await;

            current_height += mesh_inf.px_height;

            if let Some(old) = old {
                old_drawmesh.push(old);
            }
            mesh_infs.push(mesh_inf);
        }

        let total_height = current_height;

        // If lines aren't enough to fill the available buffer then start from the top
        let start_pos = if total_height < rect.h { total_height } else { rect.h };

        let mut scroll = self.scroll.get();
        if Self::clamp_scroll(&mut scroll, total_height, rect.h) {
            self.scroll.set(scroll);
        }

        let mut current_height = 0.;
        for mesh_inf in mesh_infs {
            if current_height > scroll + rect.h {
                break
            }

            // Apply scroll and scissor
            // We use the scissor for scrolling
            // Because we use the scissor, our actual rect is now rect instead of parent_rect
            let off_x = 0.;
            // This calc decides whether scroll is in terms of pages or pixels
            let off_y = (scroll + start_pos - current_height) / rect.h;
            let scale_x = 1. / rect.w;
            let scale_y = 1. / rect.h;
            let model = glam::Mat4::from_translation(glam::Vec3::new(off_x, off_y, 0.)) *
                glam::Mat4::from_scale(glam::Vec3::new(scale_x, scale_y, 1.));

            instrs.push(DrawInstruction::ApplyMatrix(model));

            instrs.push(DrawInstruction::Draw(mesh_inf.mesh));

            current_height += mesh_inf.px_height;
        }

        (instrs, old_drawmesh)
    }

    fn read_nick_colors(&self) -> Vec<Color> {
        let mut colors = vec![];
        let mut color = [0f32; 4];
        for i in 0..self.nick_colors.get_len() {
            color[i % 4] = self.nick_colors.get_f32(i).expect("prop logic err");

            if i > 0 && i % 4 == 0 {
                let color = std::mem::take(&mut color);
                colors.push(color);
            }
        }
        colors
    }

    fn select_nick_color(&self, nick: &str, nick_colors: &[Color]) -> Color {
        let mut hasher = DefaultHasher::new();
        nick.hash(&mut hasher);
        let i = hasher.finish() as usize;
        let color = nick_colors[i % nick_colors.len()];
        color
    }

    pub async fn draw(&self, sg: &SceneGraph, parent_rect: &Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::chatview", "ChatView::draw()");
        // Only used for debug messages
        let node = sg.get_node(self.node_id).unwrap();

        if let Err(err) = eval_rect(self.rect.clone(), parent_rect) {
            panic!("Node {:?} bad rect property: {}", node, err);
        }

        let Ok(mut rect) = read_rect(self.rect.clone()) else {
            panic!("Node {:?} bad rect property", node);
        };

        let timer = std::time::Instant::now();
        let (mut mesh_instrs, mut old_drawmesh) = self.regen_mesh(rect.clone()).await;
        debug!(target: "ui::chatview", "regen_mesh() took {:?}", timer.elapsed());

        let mut freed_textures = vec![];
        let mut freed_buffers = vec![];
        for old_mesh in old_drawmesh {
            freed_buffers.push(old_mesh.vertex_buffer);
            freed_buffers.push(old_mesh.index_buffer);
            //if let Some(texture_id) = old_dc.texture {
            //    freed_textures.push(texture_id);
            //}
        }

        debug!(target: "ui::chatview", "chatview rect = {:?}", rect);

        let mut instrs = vec![DrawInstruction::ApplyViewport(rect)];
        instrs.append(&mut mesh_instrs);

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                DrawCall { instrs, dcs: vec![], z_index: self.z_index.get() },
            )],
            freed_textures,
            freed_buffers,
        })
    }
}
