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

use async_trait::async_trait;
use atomic_float::AtomicF32;
use darkfi::system::CondVar;
use darkfi_serial::{serialize, Decodable};
use miniquad::{MouseButton, TouchPhase};
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::{
    collections::{HashMap, VecDeque},
    io::Read,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Weak,
    },
};

use crate::{
    gfx::{gfxtag, DrawCall, DrawInstruction, Point, Rectangle, RenderApi, Renderer, RendererSync},
    mesh::MeshBuilder,
    prop::{
        BatchGuardId, BatchGuardPtr, PropertyAtomicGuard, PropertyBool, PropertyColor,
        PropertyFloat32, PropertyPtr, PropertyRect, PropertyUint32, Role,
    },
    scene::{MethodCallSub, Pimpl, SceneNodeWeak},
    text, ExecutorPtr,
};

use super::{DrawUpdate, OnModify, UIObject};

mod shape;

const EPSILON: f32 = 0.001;
const BIG_EPSILON: f32 = 0.05;
const LONG_PRESS_EPSILON: f32 = 5.0;

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::menu", $($arg)*); } }

#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemStatus {
    Active,
    Alert,
}

#[derive(Clone)]
struct TouchInfo {
    start_scroll: f32,
    start_pos: Point,
    start_instant: std::time::Instant,
    samples: VecDeque<(std::time::Instant, f32)>,
    last_instant: std::time::Instant,
    last_y: f32,
}

#[derive(Clone)]
struct MouseClickInfo {
    start_pos: Point,
    start_instant: std::time::Instant,
}

#[derive(Clone)]
struct DragInfo {
    item_idx: usize,
    insert_idx: usize,
}

impl TouchInfo {
    fn new(start_scroll: f32, pos: Point) -> Self {
        Self {
            start_scroll,
            start_pos: pos,
            start_instant: std::time::Instant::now(),
            samples: VecDeque::from([(std::time::Instant::now(), pos.y)]),
            last_instant: std::time::Instant::now(),
            last_y: pos.y,
        }
    }

    fn push_sample(&mut self, y: f32) {
        self.samples.push_back((std::time::Instant::now(), y));

        while let Some((instant, _)) = self.samples.front() {
            if instant.elapsed().as_micros() <= 40_000 {
                break
            }
            self.samples.pop_front();
        }
    }

    fn first_sample(&self) -> Option<(f32, f32)> {
        self.samples.front().map(|(t, s)| (t.elapsed().as_micros() as f32 / 1000., *s))
    }
}

pub type MenuPtr = Arc<Menu>;

pub struct Menu {
    node: SceneNodeWeak,
    renderer: Renderer,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    root_dc_key: u64,
    content_dc_key: u64,

    is_visible: PropertyBool,
    rect: PropertyRect,
    scroll: AtomicF32,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    items: PropertyPtr,

    font_size: PropertyFloat32,
    padding: PropertyPtr,
    handle_padding: PropertyFloat32,
    text_color: PropertyColor,
    bg_color: PropertyColor,
    sep_size: PropertyFloat32,
    sep_color: PropertyColor,
    active_color: PropertyColor,
    alert_color: PropertyColor,
    fade_zone: PropertyFloat32,
    window_scale: PropertyFloat32,

    mouse_pos: SyncMutex<Point>,
    touch_info: SyncMutex<Option<TouchInfo>>,
    mouse_click_info: SyncMutex<Option<MouseClickInfo>>,
    drag_info: SyncMutex<Option<DragInfo>>,
    long_press_task: SyncMutex<Option<smol::Task<()>>>,
    weak_self: SyncMutex<Option<Weak<Self>>>,
    ex: SyncMutex<Option<ExecutorPtr>>,
    scroll_start_accel: PropertyFloat32,
    scroll_resist: PropertyFloat32,
    motion_cv: Arc<CondVar>,
    speed: AtomicF32,
    is_edit_mode: AtomicBool,

    parent_rect: SyncMutex<Option<Rectangle>>,
    item_states: SyncMutex<HashMap<String, ItemStatus>>,

    saved_items: SyncMutex<Option<Vec<String>>>,
}

impl Menu {
    pub async fn new(
        node: SceneNodeWeak,
        window_scale: PropertyFloat32,
        renderer: Renderer,
    ) -> Pimpl {
        let node_ref = &node.upgrade().unwrap();
        let is_visible = PropertyBool::wrap(node_ref, Role::Internal, "is_visible", 0).unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let items = node_ref.get_property("items").expect("Menu::items");

        let font_size = PropertyFloat32::wrap(node_ref, Role::Internal, "font_size", 0).unwrap();
        let padding = node_ref.get_property("padding").expect("Menu::padding");
        let handle_padding =
            PropertyFloat32::wrap(node_ref, Role::Internal, "handle_padding", 0).unwrap();
        let text_color = PropertyColor::wrap(node_ref, Role::Internal, "text_color").unwrap();
        let bg_color = PropertyColor::wrap(node_ref, Role::Internal, "bg_color").unwrap();
        let sep_size = PropertyFloat32::wrap(node_ref, Role::Internal, "sep_size", 0).unwrap();
        let sep_color = PropertyColor::wrap(node_ref, Role::Internal, "sep_color").unwrap();
        let active_color = PropertyColor::wrap(node_ref, Role::Internal, "active_color").unwrap();
        let alert_color = PropertyColor::wrap(node_ref, Role::Internal, "alert_color").unwrap();

        let fade_zone = PropertyFloat32::wrap(node_ref, Role::Internal, "fade_zone", 0).unwrap();

        let scroll_start_accel =
            PropertyFloat32::wrap(node_ref, Role::Internal, "scroll_start_accel", 0).unwrap();
        let scroll_resist =
            PropertyFloat32::wrap(node_ref, Role::Internal, "scroll_resist", 0).unwrap();

        let motion_cv = Arc::new(CondVar::new());

        let self_ = Arc::new(Self {
            node: node.clone(),
            renderer: renderer.clone(),
            tasks: SyncMutex::new(vec![]),
            root_dc_key: OsRng.gen(),
            content_dc_key: OsRng.gen(),
            is_visible,
            rect,
            scroll: AtomicF32::new(0.),
            z_index,
            priority,
            items,
            font_size,
            padding,
            handle_padding,
            text_color,
            bg_color,
            sep_size,
            sep_color,
            active_color,
            alert_color,
            fade_zone,
            window_scale,
            mouse_pos: SyncMutex::new(Point::new(0., 0.)),
            touch_info: SyncMutex::new(None),
            mouse_click_info: SyncMutex::new(None),
            drag_info: SyncMutex::new(None),
            long_press_task: SyncMutex::new(None),
            weak_self: SyncMutex::new(None),
            ex: SyncMutex::new(None),
            scroll_start_accel,
            scroll_resist,
            motion_cv,
            speed: AtomicF32::new(0.),
            is_edit_mode: AtomicBool::new(false),
            parent_rect: SyncMutex::new(None),
            item_states: SyncMutex::new(HashMap::new()),
            saved_items: SyncMutex::new(None),
        });

        Pimpl::Menu(self_)
    }

    /// Height of a single item
    fn get_item_height(&self) -> f32 {
        self.font_size.get() + self.padding.get_f32(1).unwrap() * 2.0
    }

    /// Save the current menu items layout
    fn save_items_layout(&self) {
        let num_items = self.items.get_len();
        let mut items = Vec::with_capacity(num_items);
        for idx in 0..num_items {
            items.push(self.items.get_str(idx).unwrap());
        }
        *self.saved_items.lock() = Some(items);
        d!("Saved menu layout with {} items", num_items);
    }

    /// Height of the content without the overscroll
    fn content_height(&self) -> f32 {
        self.items.get_len() as f32 * self.get_item_height()
    }
    fn get_selected_item_index(&self, click_y: f32) -> Option<usize> {
        let rect = self.rect.get();
        let scroll = self.scroll.load(Ordering::Relaxed);

        // Scroll is positive value so to translate click into content, we must add the scroll.
        let content_y = click_y + scroll - rect.y;
        if content_y < 0. || content_y > self.content_height() {
            return None
        }

        let item_height = self.get_item_height();
        Some((content_y / item_height) as usize)
    }

    async fn handle_selection(&self, item_idx: usize) {
        if item_idx < self.items.get_len() {
            let item_name = self.items.get_str(item_idx).unwrap();

            self.item_states.lock().remove(&item_name);

            let node = self.node.upgrade().unwrap();
            let data = serialize(&item_name);
            node.trigger("select", data).await.unwrap();
        }
    }

    async fn handle_interaction(
        &self,
        pos: Point,
        is_tap: bool,
        is_long_press_tap: bool,
        elapsed_ms: u128,
    ) {
        let is_long_press = is_long_press_tap && elapsed_ms >= 500;

        if is_long_press {
            self.save_items_layout();
            self.is_edit_mode.store(true, Ordering::Release);
            let node = self.node.upgrade().unwrap();
            node.trigger("edit_active", vec![]).await.unwrap();
            let atom = &mut self.renderer.make_guard(gfxtag!("Menu::long_press"));
            self.redraw(atom);
        } else if is_tap {
            let is_edit_mode = self.is_edit_mode.load(Ordering::Relaxed);

            if is_edit_mode {
                let font_size = self.font_size.get();
                let handle_padding = self.handle_padding.get();
                let x_half_size = font_size * 0.7;
                let x_center = handle_padding / 2.;

                if let Some(item_idx) = self.get_selected_item_index(pos.y) {
                    let item_name = self.items.get_str(item_idx).unwrap();

                    let x_min = x_center - x_half_size;
                    let x_max = x_center + x_half_size;

                    if pos.x >= x_min && pos.x <= x_max {
                        info!(target: "app::menu", "X clicked for item: {item_name}");
                        let atom = &mut self.renderer.make_guard(gfxtag!("Menu::delete_item"));
                        self.items.remove_str(atom, Role::App, item_idx).unwrap();
                        self.redraw(atom);
                    } else {
                        self.handle_selection(item_idx).await;
                    }
                }
            } else if let Some(item_idx) = self.get_selected_item_index(pos.y) {
                self.handle_selection(item_idx).await;
            }
        }
    }

    fn get_draw_calls(
        &self,
        atom: &mut PropertyAtomicGuard,
        parent_rect: Rectangle,
    ) -> Option<DrawUpdate> {
        self.rect.eval(atom, &parent_rect).ok()?;
        let rect = self.rect.get();

        let mut instrs = vec![];

        let scroll = self.scroll.load(Ordering::Relaxed);
        let item_height = self.get_item_height();
        let font_size = self.font_size.get();
        let padding_x = self.padding.get_f32(0).unwrap();
        let padding_y = self.padding.get_f32(1).unwrap();
        let handle_padding = self.handle_padding.get();
        let text_color = self.text_color.get();
        let active_color = self.active_color.get();
        let alert_color = self.alert_color.get();
        let bg_color = self.bg_color.get();
        let sep_size = self.sep_size.get();
        let sep_color = self.sep_color.get();
        let fade_distance = self.fade_zone.get();
        let window_scale = self.window_scale.get();

        let num_items = self.items.get_len();

        // Get items and reorder if dragging
        let mut items_list = {
            let mut items = vec![];
            for idx in 0..num_items {
                items.push(self.items.get_str(idx).unwrap());
            }
            items
        };

        if let Some(ref drag_info) = self.drag_info.lock().as_ref() {
            if drag_info.item_idx != drag_info.insert_idx {
                let item = items_list.remove(drag_info.item_idx);
                items_list.insert(drag_info.insert_idx, item);
            }
        }

        // Draw single background mesh for the entire menu
        let content_height = num_items as f32 * item_height;

        let mut bg_mesh = MeshBuilder::new(gfxtag!("menu_bg"));
        bg_mesh.draw_filled_box(&Rectangle::new(0., 0., rect.w, content_height), bg_color);
        let bg_mesh = bg_mesh.alloc(&self.renderer).draw_untextured();

        instrs.push(DrawInstruction::Draw(bg_mesh));

        // Separator line mesh
        let mut sep_mesh = MeshBuilder::new(gfxtag!("menu_sep"));
        sep_mesh.draw_filled_box(&Rectangle::new(0., 0., rect.w, sep_size), sep_color);
        let sep_mesh = sep_mesh.alloc(&self.renderer).draw_untextured();

        let item_states = self.item_states.lock();
        let is_edit_mode = self.is_edit_mode.load(Ordering::Relaxed);
        let edit_offset = if is_edit_mode { handle_padding } else { 0.0 };

        // Create X mesh for edit mode
        let x_mesh =
            if is_edit_mode { Some(shape::make_x(&self.renderer, font_size)) } else { None };

        let mut edit_instrs = vec![];
        if is_edit_mode {
            let item_center_y = item_height / 2.0;
            edit_instrs.push(DrawInstruction::Move(Point::new(handle_padding / 2., item_center_y)));
            edit_instrs.push(DrawInstruction::Draw(shape::make_x(&self.renderer, font_size)));
            edit_instrs
                .push(DrawInstruction::Move(Point::new(-handle_padding / 2., -item_center_y)));

            let rhs = rect.w - handle_padding / 2.;
            edit_instrs.push(DrawInstruction::Move(Point::new(rhs, item_center_y)));
            edit_instrs.push(DrawInstruction::Draw(shape::make_hammy(&self.renderer, font_size)));
            edit_instrs.push(DrawInstruction::Move(Point::new(-rhs, -item_center_y)));
        }

        for idx in 0..num_items {
            let item_text = items_list[idx].clone();

            let base_color = match item_states.get(&item_text) {
                Some(ItemStatus::Active) => active_color,
                Some(ItemStatus::Alert) => alert_color,
                _ => text_color,
            };

            // Apply fade effect in the configured fade zone
            let item_y = idx as f32 * item_height - scroll;
            let fade_zone_start = rect.h - fade_distance;
            let color = if item_y >= fade_zone_start {
                let fade_factor =
                    1.0 - ((item_y - fade_zone_start) / fade_distance).clamp(0.0, 1.0);
                let mut faded = base_color;
                faded[3] *= fade_factor;
                faded
            } else {
                base_color
            };

            instrs.append(&mut edit_instrs.clone());

            // Draw text
            let layout = text::make_layout(
                &item_text,
                color,
                font_size,
                1.0,
                window_scale,
                Some(rect.w - padding_x * 2.),
                &[],
            );

            let text_instr = text::render_layout(&layout, &self.renderer, gfxtag!("menu_text"));

            instrs.push(DrawInstruction::Move(Point::new(padding_x + edit_offset, padding_y)));
            instrs.extend(text_instr);
            instrs.push(DrawInstruction::Move(Point::new(
                -padding_x - edit_offset,
                font_size + padding_y,
            )));

            // Draw separator (except for last item)
            if idx < num_items - 1 {
                instrs.push(DrawInstruction::Draw(sep_mesh.clone()));
            }
        }

        Some(DrawUpdate {
            key: self.root_dc_key,
            draw_calls: vec![
                (
                    self.root_dc_key,
                    DrawCall {
                        instrs: vec![
                            DrawInstruction::ApplyView(rect),
                            DrawInstruction::Move(Point::new(0., -scroll)),
                        ],
                        dcs: vec![self.content_dc_key],
                        z_index: self.z_index.get(),
                        debug_str: "menu_root",
                    },
                ),
                (
                    self.content_dc_key,
                    DrawCall {
                        instrs,
                        dcs: vec![],
                        z_index: self.z_index.get(),
                        debug_str: "menu_content",
                    },
                ),
            ],
        })
    }

    fn redraw(&self, atom: &mut PropertyAtomicGuard) {
        let Some(parent_rect) = self.parent_rect.lock().clone() else { return };
        let Some(draw_update) = self.get_draw_calls(atom, parent_rect) else { return };
        self.renderer.replace_draw_calls(Some(atom.batch_id), draw_update.draw_calls);
    }

    fn redraw_scroll<R: RenderApi>(&self, renderer: &R) {
        let rect = self.rect.get();
        let scroll = self.scroll.load(Ordering::Relaxed);

        // Only recreate root with updated scroll position
        let root_instrs =
            vec![DrawInstruction::ApplyView(rect), DrawInstruction::Move(Point::new(0., -scroll))];

        let root_dc = DrawCall {
            instrs: root_instrs,
            dcs: vec![self.content_dc_key],
            z_index: self.z_index.get(),
            debug_str: "menu_root",
        };

        let draw_calls = vec![(self.root_dc_key, root_dc)];
        renderer.replace_draw_calls(None, draw_calls);
    }

    fn scrollview(&self, scroll: f32) {
        let item_height = self.get_item_height();
        let num_items = self.items.get_len() as f32;
        let content_height = num_items * item_height;

        let rect = self.rect.get();
        let max_scroll = (content_height - rect.h).max(0.);

        // Allow 50% overscroll past the end of the content
        let overscroll = rect.h * 0.5;
        let scroll = scroll.clamp(0., max_scroll + overscroll);
        self.scroll.store(scroll, Ordering::Relaxed);
    }

    fn start_scroll(&self, delta: f32) {
        let accel = self.scroll_start_accel.get();
        self.speed.store(delta * accel, Ordering::Relaxed);
        self.motion_cv.notify();
    }

    async fn handle_movement(&self) {
        let resist = self.scroll_resist.get();

        loop {
            self.motion_cv.wait().await;

            let mut speed = self.speed.load(Ordering::Relaxed);
            if speed.abs() < EPSILON {
                continue
            }

            while speed.abs() >= EPSILON {
                let scroll = self.scroll.load(Ordering::Relaxed);
                self.scrollview(scroll + speed);
                self.redraw_scroll(&self.renderer);
                speed *= resist;
                self.speed.store(speed, Ordering::Relaxed);
                darkfi::system::msleep(16).await;
            }

            self.speed.store(0., Ordering::Relaxed);
        }
    }

    fn end_touch_phase(&self, touch_y: f32) {
        let touch_info = std::mem::take(&mut *self.touch_info.lock());
        let info = touch_info.unwrap();

        if let Some((dt, _)) = info.first_sample() {
            if dt > EPSILON {
                let velocity = (touch_y - info.start_pos.y) / dt;
                self.start_scroll(-velocity);
            }
        }
    }

    async fn process_mark_active_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("Event relayer closed");
            return false
        };

        d!("method called: mark_active({method_call:?})");
        assert!(method_call.send_res.is_none());

        fn decode_data(data: &[u8]) -> std::io::Result<String> {
            use std::io::Cursor;
            let mut cur = Cursor::new(&data);
            let item_name = String::decode(&mut cur)?;
            Ok(item_name)
        }

        let Ok(item_name) = decode_data(&method_call.data) else {
            d!("mark_active() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            d!("Self destroyed");
            return true
        };

        self_.item_states.lock().insert(item_name, ItemStatus::Active);
        let atom = &mut self_.renderer.make_guard(gfxtag!("Menu::mark_active"));
        self_.redraw(atom);

        true
    }

    async fn process_mark_alert_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("Event relayer closed");
            return false
        };

        d!("method called: mark_alert({method_call:?})");
        assert!(method_call.send_res.is_none());

        fn decode_data(data: &[u8]) -> std::io::Result<String> {
            use std::io::Cursor;
            let mut cur = Cursor::new(&data);
            let item_name = String::decode(&mut cur)?;
            Ok(item_name)
        }

        let Ok(item_name) = decode_data(&method_call.data) else {
            d!("mark_alert() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            d!("Self destroyed");
            return true
        };

        self_.item_states.lock().insert(item_name, ItemStatus::Alert);
        let atom = &mut self_.renderer.make_guard(gfxtag!("Menu::mark_alert"));
        self_.redraw(atom);

        true
    }

    /// Cancels edit mode changes, reverting any modifications made during edit mode
    async fn process_cancel_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("Event relayer closed");
            return false
        };

        d!("method called: cancel({method_call:?})");
        assert!(method_call.send_res.is_none());

        let Some(self_) = me.upgrade() else {
            d!("Self destroyed");
            return true
        };

        // Restore the saved items if they exist
        let saved = self_.saved_items.lock().take();
        if let Some(items) = saved {
            let atom = &mut self_.renderer.make_guard(gfxtag!("Menu::cancel_edit"));

            // Clear current items
            let current_len = self_.items.get_len();
            for _ in 0..current_len {
                self_.items.remove_str(atom, Role::App, 0).unwrap();
            }

            // Restore saved items
            for (idx, item) in items.iter().enumerate() {
                self_.items.insert_str(atom, Role::App, idx, item).unwrap();
            }

            d!("cancel: restored {} items", items.len());
        }

        // Exit edit mode
        self_.is_edit_mode.store(false, Ordering::Release);
        let atom = &mut self_.renderer.make_guard(gfxtag!("Menu::cancel_edit"));
        self_.redraw(atom);

        true
    }

    /// Accepts edit mode changes, finalizing any modifications made during edit mode
    async fn process_done_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("Event relayer closed");
            return false
        };

        d!("method called: done({method_call:?})");
        assert!(method_call.send_res.is_none());

        let Some(self_) = me.upgrade() else {
            d!("Self destroyed");
            return true
        };

        // Clear the saved items since we're finalizing the changes
        *self_.saved_items.lock() = None;

        self_.is_edit_mode.store(false, Ordering::Release);
        let atom = &mut self_.renderer.make_guard(gfxtag!("Menu::done_edit"));
        self_.redraw(atom);

        true
    }
}

#[async_trait]
impl UIObject for Menu {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        *self.weak_self.lock() = Some(Arc::downgrade(&self));
        *self.ex.lock() = Some(ex.clone());
        let me = Arc::downgrade(&self);
        let node_ref = &self.node.upgrade().unwrap();

        let me2 = me.clone();
        let cv = self.motion_cv.clone();
        let motion_task = ex.spawn(async move {
            loop {
                cv.wait().await;
                let Some(self_) = me2.upgrade() else {
                    panic!("Self destroyed before motion_task stopped");
                };
                self_.handle_movement().await;
                cv.reset();
            }
        });

        let method_sub = node_ref.subscribe_method_call("mark_active").unwrap();
        let me2 = me.clone();
        let mark_active_task =
            ex.spawn(
                async move { while Self::process_mark_active_method(&me2, &method_sub).await {} },
            );

        let method_sub = node_ref.subscribe_method_call("mark_alert").unwrap();
        let me2 = me.clone();
        let mark_alert_task =
            ex.spawn(
                async move { while Self::process_mark_alert_method(&me2, &method_sub).await {} },
            );

        let method_sub = node_ref.subscribe_method_call("cancel_edit").unwrap();
        let me2 = me.clone();
        let cancel_task =
            ex.spawn(async move { while Self::process_cancel_method(&me2, &method_sub).await {} });

        let method_sub = node_ref.subscribe_method_call("done_edit").unwrap();
        let me2 = me.clone();
        let done_task =
            ex.spawn(async move { while Self::process_done_method(&me2, &method_sub).await {} });

        let mut on_modify = OnModify::new(ex, self.node.clone(), me.clone());

        async fn redraw(self_: Arc<Menu>, batch: BatchGuardPtr) {
            let atom = &mut batch.spawn();
            self_.redraw(atom);
        }

        on_modify.when_change(self.items.clone(), redraw);
        on_modify.when_change(self.rect.prop(), redraw);
        on_modify.when_change(self.font_size.prop(), redraw);
        on_modify.when_change(self.padding.clone(), redraw);
        on_modify.when_change(self.text_color.prop(), redraw);
        on_modify.when_change(self.bg_color.prop(), redraw);
        on_modify.when_change(self.sep_size.prop(), redraw);
        on_modify.when_change(self.sep_color.prop(), redraw);

        let mut tasks =
            vec![motion_task, mark_active_task, mark_alert_task, cancel_task, done_task];
        tasks.append(&mut on_modify.tasks);
        *self.tasks.lock() = tasks;
    }

    fn stop(&self) {
        *self.tasks.lock() = vec![];
    }

    async fn draw(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        *self.parent_rect.lock() = Some(parent_rect);
        self.get_draw_calls(atom, parent_rect)
    }

    async fn handle_mouse_btn_down(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if btn != MouseButton::Left {
            return false
        }

        let rect = self.rect.get();
        if !rect.contains(mouse_pos) {
            return false
        }

        let is_edit_mode = self.is_edit_mode.load(Ordering::Relaxed);

        if is_edit_mode {
            let font_size = self.font_size.get();
            let handle_padding = self.handle_padding.get();
            let hammy_half_size = font_size * 0.7;
            let hammy_center = rect.w - handle_padding / 2.;

            let hammy_min = hammy_center - hammy_half_size;
            let hammy_max = hammy_center + hammy_half_size;

            if mouse_pos.x >= hammy_min && mouse_pos.x <= hammy_max {
                if let Some(item_idx) = self.get_selected_item_index(mouse_pos.y) {
                    *self.drag_info.lock() = Some(DragInfo { item_idx, insert_idx: item_idx });
                    info!(target: "app::menu", "Dragging item: {}", item_idx);
                }
            }
        }

        *self.mouse_click_info.lock() =
            Some(MouseClickInfo { start_pos: mouse_pos, start_instant: std::time::Instant::now() });

        // Spawn a task to detect long press
        let weak_self = self.weak_self.lock().clone().unwrap();
        let start_pos = mouse_pos;

        let ex = self.ex.lock().clone().unwrap();
        let long_press_task = ex.spawn(async move {
            darkfi::system::msleep(500).await;

            let Some(arc_self) = weak_self.upgrade() else { return };
            let current_mouse_pos = arc_self.mouse_pos.lock().clone();
            let click_info = arc_self.mouse_click_info.lock().clone();

            // Check if button is still held and movement is within threshold
            if let Some(info) = click_info {
                let movement_dist = ((current_mouse_pos.x - start_pos.x).powi(2) +
                    (current_mouse_pos.y - start_pos.y).powi(2))
                .sqrt();

                if movement_dist < LONG_PRESS_EPSILON {
                    // Long press detected, trigger edit mode
                    arc_self.save_items_layout();
                    arc_self.is_edit_mode.store(true, Ordering::Release);
                    let node = arc_self.node.upgrade().unwrap();
                    node.trigger("edit_active", vec![]).await.unwrap();
                    let atom = &mut arc_self.renderer.make_guard(gfxtag!("Menu::long_press"));
                    arc_self.redraw(atom);
                }
            }
        });

        *self.long_press_task.lock() = Some(long_press_task);

        false
    }

    async fn handle_mouse_btn_up(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if btn != MouseButton::Left {
            return false
        }

        // Apply drag reorder if we were dragging
        let drag = self.drag_info.lock().take();
        if let Some(drag_info) = drag {
            if drag_info.item_idx != drag_info.insert_idx {
                let item = self.items.get_str(drag_info.item_idx).unwrap();
                let atom = &mut self.renderer.make_guard(gfxtag!("Menu::reorder_item"));
                self.items.remove_str(atom, Role::App, drag_info.item_idx).unwrap();
                let insert_idx = drag_info.insert_idx;
                self.items.insert_str(atom, Role::App, insert_idx, &item).unwrap();
                info!(target: "app::menu", "Reordered item {} to {}", drag_info.item_idx, insert_idx);
            }
            return true
        }

        // Cancel the long press detection task
        let task = self.long_press_task.lock().take();
        if let Some(task) = task {
            task.cancel().await;
        }

        let click_info = self.mouse_click_info.lock().take();
        let Some(info) = click_info else { return false };

        let is_click = (mouse_pos.y - info.start_pos.y).abs() < BIG_EPSILON;
        let movement_dist = ((mouse_pos.x - info.start_pos.x).powi(2) +
            (mouse_pos.y - info.start_pos.y).powi(2))
        .sqrt();
        let is_long_press_tap = movement_dist < LONG_PRESS_EPSILON;
        let elapsed = info.start_instant.elapsed().as_millis();

        self.handle_interaction(mouse_pos, is_click, is_long_press_tap, elapsed).await;

        true
    }

    async fn handle_mouse_wheel(&self, wheel_pos: Point) -> bool {
        let rect = self.rect.get();
        let mouse_pos = self.mouse_pos.lock().clone();

        if !rect.contains(mouse_pos) {
            return false
        }

        self.start_scroll(-wheel_pos.y);
        true
    }

    async fn handle_mouse_move(&self, mouse_pos: Point) -> bool {
        *self.mouse_pos.lock() = mouse_pos;

        let mut should_redraw = false;

        if self.drag_info.lock().is_some() {
            if let Some(insert_idx) = self.get_selected_item_index(mouse_pos.y) {
                let mut drag = self.drag_info.lock();
                if let Some(d) = drag.as_mut() {
                    if d.insert_idx != insert_idx {
                        d.insert_idx = insert_idx;
                        info!(target: "app::menu", "insert_idx changed to: {}", insert_idx);
                        should_redraw = true;
                    }
                }
            }
        }

        if should_redraw {
            let atom = &mut self.renderer.make_guard(gfxtag!("Menu::drag_update"));
            self.redraw(atom);
        }

        false
    }

    fn handle_touch_sync(
        &self,
        renderer: &RendererSync,
        phase: TouchPhase,
        id: u64,
        touch_pos: Point,
    ) -> bool {
        if id != 0 {
            return false
        }

        match phase {
            TouchPhase::Started => {
                let rect = self.rect.get();
                if !rect.contains(touch_pos) {
                    *self.touch_info.lock() = None;
                    return false
                }

                let is_edit_mode = self.is_edit_mode.load(Ordering::Relaxed);

                if is_edit_mode {
                    let font_size = self.font_size.get();
                    let handle_padding = self.handle_padding.get();
                    let hammy_half_size = font_size * 0.7;
                    let hammy_center = rect.w - handle_padding / 2.;

                    let hammy_min = hammy_center - hammy_half_size;
                    let hammy_max = hammy_center + hammy_half_size;

                    if touch_pos.x >= hammy_min && touch_pos.x <= hammy_max {
                        if let Some(item_idx) = self.get_selected_item_index(touch_pos.y) {
                            *self.drag_info.lock() =
                                Some(DragInfo { item_idx, insert_idx: item_idx });
                            info!(target: "app::menu", "Dragging item: {}", item_idx);
                        }
                    }
                }

                *self.touch_info.lock() =
                    Some(TouchInfo::new(self.scroll.load(Ordering::Relaxed), touch_pos));
                true
            }

            TouchPhase::Moved => {
                let mut should_redraw = false;

                if self.drag_info.lock().is_some() {
                    if let Some(insert_idx) = self.get_selected_item_index(touch_pos.y) {
                        let mut drag = self.drag_info.lock();
                        if let Some(d) = drag.as_mut() {
                            if d.insert_idx != insert_idx {
                                d.insert_idx = insert_idx;
                                info!(target: "app::menu", "insert_idx changed to: {}", insert_idx);
                                should_redraw = true;
                            }
                        }
                    }
                }

                if should_redraw {
                    let atom = &mut self.renderer.make_guard(gfxtag!("Menu::drag_update"));
                    self.redraw(atom);
                }

                let scroll = {
                    let mut touch_info = self.touch_info.lock();
                    let Some(info) = &mut *touch_info else { return false };

                    info.last_y = touch_pos.y;
                    info.push_sample(touch_pos.y);

                    let last_elapsed = info.last_instant.elapsed().as_millis();
                    if last_elapsed <= 20 {
                        return true
                    }
                    info.last_instant = std::time::Instant::now();

                    let dist = touch_pos.y - info.start_pos.y;
                    if dist.abs() < BIG_EPSILON {
                        return true
                    }

                    info.start_scroll - dist
                };

                self.scrollview(scroll);
                self.redraw_scroll(renderer);
                true
            }

            // Use async handler instead
            TouchPhase::Ended | TouchPhase::Cancelled => false,
        }
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_pos: Point) -> bool {
        if id != 0 {
            return false
        }

        match phase {
            // Should be handled by handle_touch_sync
            TouchPhase::Started | TouchPhase::Moved => false,

            TouchPhase::Ended | TouchPhase::Cancelled => {
                let drag = self.drag_info.lock().take();
                if let Some(drag_info) = drag {
                    if drag_info.item_idx != drag_info.insert_idx {
                        let item = self.items.get_str(drag_info.item_idx).unwrap();
                        let atom = &mut self.renderer.make_guard(gfxtag!("Menu::reorder_item"));
                        self.items.remove_str(atom, Role::App, drag_info.item_idx).unwrap();
                        let insert_idx = drag_info.insert_idx;
                        self.items.insert_str(atom, Role::App, insert_idx, &item).unwrap();
                        info!(target: "app::menu", "Reordered item {} to {}", drag_info.item_idx, insert_idx);
                    }
                    return true
                }

                let (is_tap, is_long_press_tap, elapsed) = {
                    let touch_info = self.touch_info.lock();
                    let Some(info) = &*touch_info else { return true };

                    let is_tap = (touch_pos.y - info.start_pos.y).abs() < BIG_EPSILON;
                    let movement_dist = ((touch_pos.x - info.start_pos.x).powi(2) +
                        (touch_pos.y - info.start_pos.y).powi(2))
                    .sqrt();
                    let is_long_press_tap = movement_dist < LONG_PRESS_EPSILON;
                    let elapsed = info.start_instant.elapsed().as_millis();
                    (is_tap, is_long_press_tap, elapsed)
                };

                self.handle_interaction(touch_pos, is_tap, is_long_press_tap, elapsed).await;

                self.end_touch_phase(touch_pos.y);
                true
            }
        }
    }
}
