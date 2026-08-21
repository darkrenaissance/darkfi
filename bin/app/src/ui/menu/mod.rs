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
use async_trait::async_trait;
use atomic_float::AtomicF32;
use darkfi::system::CondVar;
use darkfi_serial::{serialize, Decodable};
use miniquad::{MouseButton, TouchPhase};
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::Read,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Weak,
    },
};

use crate::{
    gfx::{gfxtag, DrawCall, DrawInstruction, DrawMesh, Point, Rectangle, Renderer, Vertex},
    mesh::MeshBuilder,
    prop::{
        PropertyAtomicGuard, PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr,
        PropertyRect, PropertyUint32, Role,
    },
    scene::{MethodCallSub, Pimpl, SceneNodeWeak},
    text, ExecutorPtr,
};

use super::{DrawUpdate, OnModify, RedrawTrigger, UIObject};

mod shape;

const EPSILON: f32 = 0.001;
const BIG_EPSILON: f32 = 0.05;
const LONG_PRESS_EPSILON: f32 = 5.0;

#[cfg(target_os = "android")]
const MENU_ICON_OFFSET: f32 = 55.;

#[cfg(not(target_os = "android"))]
const MENU_ICON_OFFSET: f32 = 24.;

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::menu", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::menu", $($arg)*); } }

#[derive(Clone)]
struct TouchInfo {
    start_scroll: f32,
    start_pos: Point,
    start_instant: std::time::Instant,
    samples: VecDeque<(std::time::Instant, f32)>,
    last_instant: std::time::Instant,
    last_pos: Point,
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
            last_pos: pos,
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
    /// Weak self-reference so handlers can spawn detached tasks.
    me: Weak<Self>,
    ex: ExecutorPtr,
    renderer: Renderer,
    redraw: RedrawTrigger,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    root_dc_key: u64,
    content_dc_key: u64,
    bg_dc_key: u64,

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
    role1_color: PropertyColor,
    role1_group: PropertyPtr,
    role2_color: PropertyColor,
    role2_group: PropertyPtr,
    fade_zone: PropertyFloat32,
    window_scale: PropertyFloat32,

    mouse_pos: SyncMutex<Point>,
    touch_info: SyncMutex<Option<TouchInfo>>,
    mouse_click_info: SyncMutex<Option<MouseClickInfo>>,
    drag_info: SyncMutex<Option<DragInfo>>,
    long_press_task: SyncMutex<Option<smol::Task<()>>>,
    scroll_start_accel: PropertyFloat32,
    scroll_resist: PropertyFloat32,
    motion_cv: Arc<CondVar>,
    speed: AtomicF32,
    is_edit_mode: AtomicBool,

    parent_rect: SyncMutex<Option<Rectangle>>,
    saved_items: SyncMutex<Option<Vec<String>>>,

    /// Opaque per-item text instructions, indexed by item position.
    item_instrs: SyncMutex<Vec<Option<Vec<DrawInstruction>>>>,
    /// Content instructions tagged with the scroll offset they were
    /// assembled for. `None` means stale.
    content_cache: SyncMutex<Option<(f32, Vec<DrawInstruction>)>>,
    /// Viewport-anchored background meshes: opaque part plus fade
    /// gradient. `None` means stale.
    bg_meshes: SyncMutex<Option<(DrawMesh, Option<DrawMesh>)>>,
}

impl Menu {
    pub async fn new(
        node: SceneNodeWeak,
        window_scale: PropertyFloat32,
        renderer: Renderer,
        redraw: RedrawTrigger,
        ex: ExecutorPtr,
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
        let role1_color = PropertyColor::wrap(node_ref, Role::Internal, "role1_color").unwrap();
        let role1_group = node_ref.get_property("role1_group").unwrap();
        let role2_color = PropertyColor::wrap(node_ref, Role::Internal, "role2_color").unwrap();
        let role2_group = node_ref.get_property("role2_group").unwrap();

        let fade_zone = PropertyFloat32::wrap(node_ref, Role::Internal, "fade_zone", 0).unwrap();

        let scroll_start_accel =
            PropertyFloat32::wrap(node_ref, Role::Internal, "scroll_start_accel", 0).unwrap();
        let scroll_resist =
            PropertyFloat32::wrap(node_ref, Role::Internal, "scroll_resist", 0).unwrap();

        let motion_cv = Arc::new(CondVar::new());

        let self_ = Arc::new_cyclic(|me| Self {
            node: node.clone(),
            me: me.clone(),
            ex,
            renderer: renderer.clone(),
            redraw,
            tasks: SyncMutex::new(vec![]),
            root_dc_key: OsRng.gen(),
            content_dc_key: OsRng.gen(),
            bg_dc_key: OsRng.gen(),
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
            role1_color,
            role1_group,
            role2_color,
            role2_group,
            fade_zone,
            window_scale,
            mouse_pos: SyncMutex::new(Point::new(0., 0.)),
            touch_info: SyncMutex::new(None),
            mouse_click_info: SyncMutex::new(None),
            drag_info: SyncMutex::new(None),
            long_press_task: SyncMutex::new(None),
            scroll_start_accel,
            scroll_resist,
            motion_cv,
            speed: AtomicF32::new(0.),
            is_edit_mode: AtomicBool::new(false),
            parent_rect: SyncMutex::new(None),
            saved_items: SyncMutex::new(None),
            item_instrs: SyncMutex::new(vec![]),
            content_cache: SyncMutex::new(None),
            bg_meshes: SyncMutex::new(None),
        });

        Pimpl::Menu(self_)
    }

    /// Invalidate all cached draw artifacts. Locks are taken one at a
    /// time, never nested, so this cannot deadlock against `draw()`.
    fn invalidate_draw(&self) {
        *self.item_instrs.lock() = vec![];
        *self.content_cache.lock() = None;
        *self.bg_meshes.lock() = None;
    }

    /// Height of a single item
    fn get_item_height(&self) -> f32 {
        self.font_size.get() + self.padding.get_f32(1).unwrap() * 2.0
    }

    /// Save the current menu items layout
    fn save_items_layout(&self) {
        let items = self.items.get_str_vec().unwrap();
        *self.saved_items.lock() = Some(items);
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
            let node = self.node.upgrade().unwrap();
            let item_name = self.items.get_str(item_idx).unwrap();
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
        let is_long_press = is_long_press_tap && elapsed_ms >= long_press_timeout() as u128;

        if is_long_press {
            if !self.is_edit_mode.load(Ordering::Relaxed) {
                self.save_items_layout();
                self.is_edit_mode.store(true, Ordering::Release);
                let node = self.node.upgrade().unwrap();
                node.trigger("edit_active", vec![]).await.unwrap();
                self.invalidate_draw();
                self.redraw.trigger();
            }
        } else if is_tap {
            let is_edit_mode = self.is_edit_mode.load(Ordering::Relaxed);

            if is_edit_mode {
                let font_size = self.font_size.get();
                let x_half_size = font_size * 0.85;
                let x_center = MENU_ICON_OFFSET + font_size * 0.4;

                if let Some(item_idx) = self.get_selected_item_index(pos.y) {
                    let item_name = self.items.get_str(item_idx).unwrap();

                    let x_min = x_center - x_half_size;
                    let x_max = x_center + x_half_size;

                    if pos.x >= x_min && pos.x <= x_max {
                        info!(target: "app::menu", "X clicked for item: {item_name}");
                        let atom = &mut self.renderer.make_guard(gfxtag!("Menu::delete_item"));
                        self.items.remove_str(atom, Role::App, item_idx).unwrap();
                    } else {
                        self.handle_selection(item_idx).await;
                    }
                }
            } else if let Some(item_idx) = self.get_selected_item_index(pos.y) {
                self.handle_selection(item_idx).await;
            }
        }
    }

    /// Fade alpha for a content-space y position at the given scroll:
    /// 1 above the fade zone, decreasing linearly to 0 at the bottom
    /// edge of the viewport.
    fn fade_factor(&self, rect: &Rectangle, content_y: f32, scroll: f32) -> f32 {
        let fade_distance = self.fade_zone.get();
        if fade_distance <= EPSILON {
            return 1.0
        }

        let viewport_y = content_y - scroll;
        let fade_zone_start = rect.h - fade_distance;
        if viewport_y <= fade_zone_start {
            return 1.0
        }

        1.0 - ((viewport_y - fade_zone_start) / fade_distance).clamp(0.0, 1.0)
    }

    /// Render a single item's text as draw instructions with the given
    /// color.
    fn make_text_instrs(
        &self,
        item_text: &str,
        color: crate::mesh::Color,
        rect: &Rectangle,
        font_size: f32,
        window_scale: f32,
        padding_x: f32,
    ) -> Vec<DrawInstruction> {
        let layout = text::make_layout(
            item_text,
            color,
            font_size,
            1.0,
            window_scale,
            Some(rect.w - padding_x * 2.),
            &[],
        );

        text::render_layout(&layout, &self.renderer, gfxtag!("menu_text"))
    }

    /// Assemble the content draw instructions for the given scroll
    /// offset. Items away from the fade zone reuse cached opaque text
    /// instructions; items intersecting it are rebuilt with faded
    /// colors. Separators are baked into one mesh with per-separator
    /// alpha at absolute content positions.
    fn assemble_content(&self, rect: &Rectangle, scroll: f32) -> Vec<DrawInstruction> {
        let mut instrs = vec![];

        let item_height = self.get_item_height();
        let font_size = self.font_size.get();
        let padding_x = self.padding.get_f32(0).unwrap();
        let padding_y = self.padding.get_f32(1).unwrap();
        let handle_padding = self.handle_padding.get();
        let text_color = self.text_color.get();
        let role1_color = self.role1_color.get();
        let role2_color = self.role2_color.get();
        let sep_size = self.sep_size.get();
        let sep_color = self.sep_color.get();
        let window_scale = self.window_scale.get();

        let role1_set: HashSet<String> =
            self.role1_group.get_str_vec().unwrap_or_default().into_iter().collect();
        let role2_set: HashSet<String> =
            self.role2_group.get_str_vec().unwrap_or_default().into_iter().collect();

        let num_items = self.items.get_len();

        // Get items and reorder if dragging
        let items_list = {
            let mut items = vec![];
            for idx in 0..num_items {
                items.push(self.items.get_str(idx).unwrap());
            }

            if let Some(ref drag_info) = self.drag_info.lock().as_ref() {
                if drag_info.item_idx != drag_info.insert_idx {
                    let item = items.remove(drag_info.item_idx);
                    items.insert(drag_info.insert_idx, item);
                }
            }
            items
        };

        // Single separator mesh covering all separators, each faded by
        // its on-screen position. Drawn first while the cursor sits at
        // the content origin, so baked positions map directly.
        if num_items > 1 {
            let mut sep_builder = MeshBuilder::new(gfxtag!("menu_sep"));
            let uv = [0., 0.];

            for idx in 0..num_items - 1 {
                let y = (idx + 1) as f32 * item_height;
                let factor = self.fade_factor(rect, y, scroll);
                if factor <= 0.0 {
                    continue
                }

                let color = [sep_color[0], sep_color[1], sep_color[2], sep_color[3] * factor];
                sep_builder.append(
                    vec![
                        Vertex { pos: [0., y], color, uv },
                        Vertex { pos: [rect.w, y], color, uv },
                        Vertex { pos: [0., y + sep_size], color, uv },
                        Vertex { pos: [rect.w, y + sep_size], color, uv },
                    ],
                    vec![0, 2, 1, 1, 2, 3],
                );
            }

            let sep_mesh = sep_builder.alloc(&self.renderer).draw_untextured();
            instrs.push(DrawInstruction::Draw(sep_mesh));
        }

        let is_edit_mode = self.is_edit_mode.load(Ordering::Relaxed);
        let edit_offset = if is_edit_mode { handle_padding } else { 0.0 };

        let mut edit_instrs = vec![];
        if is_edit_mode {
            let item_center_y = item_height / 2.0;
            let x_center = MENU_ICON_OFFSET + font_size * 0.4;
            let rhs = rect.w - MENU_ICON_OFFSET - font_size * 0.56;

            edit_instrs.push(DrawInstruction::Move(Point::new(x_center, item_center_y)));
            edit_instrs.push(DrawInstruction::Draw(shape::make_x(&self.renderer, font_size)));
            edit_instrs.push(DrawInstruction::Move(Point::new(-x_center, -item_center_y)));

            edit_instrs.push(DrawInstruction::Move(Point::new(rhs, item_center_y)));
            edit_instrs.push(DrawInstruction::Draw(shape::make_hammy(&self.renderer, font_size)));
            edit_instrs.push(DrawInstruction::Move(Point::new(-rhs, -item_center_y)));
        }

        let mut item_instrs = self.item_instrs.lock();
        if item_instrs.len() != num_items {
            item_instrs.clear();
            item_instrs.resize(num_items, None);
        }

        for idx in 0..num_items {
            let item_text = items_list[idx].clone();

            let base_color = if role2_set.contains(&item_text) {
                role2_color
            } else if role1_set.contains(&item_text) {
                role1_color
            } else {
                text_color
            };

            let factor = self.fade_factor(rect, idx as f32 * item_height, scroll);
            if factor <= 0.0 {
                continue
            }

            instrs.append(&mut edit_instrs.clone());

            // Use a fraction of edit_offset for the label position to reduce gap from X icon
            let label_edit_offset = edit_offset * 0.62;
            instrs
                .push(DrawInstruction::Move(Point::new(padding_x + label_edit_offset, padding_y)));

            if factor >= 1.0 {
                if item_instrs[idx].is_none() {
                    item_instrs[idx] = Some(self.make_text_instrs(
                        &item_text,
                        base_color,
                        rect,
                        font_size,
                        window_scale,
                        padding_x,
                    ));
                }
                instrs.extend(item_instrs[idx].clone().unwrap());
            } else {
                let mut faded = base_color;
                faded[3] *= factor;
                instrs.extend(self.make_text_instrs(
                    &item_text,
                    faded,
                    rect,
                    font_size,
                    window_scale,
                    padding_x,
                ));
            }

            instrs.push(DrawInstruction::Move(Point::new(
                -padding_x - label_edit_offset,
                font_size + padding_y,
            )));
        }

        instrs
    }

    /// Build the viewport-anchored background meshes: an opaque quad
    /// above the fade zone and a gradient quad fading to fully
    /// transparent at the bottom edge, so the parent background shows
    /// through.
    fn make_bg_meshes(&self, rect: &Rectangle) -> (DrawMesh, Option<DrawMesh>) {
        let fade_distance = self.fade_zone.get();
        let content_height = self.content_height();
        let bg_color = self.bg_color.get();

        let fade_top = if fade_distance > EPSILON { rect.h - fade_distance } else { rect.h };
        let main_bottom = fade_top.min(content_height).min(rect.h);

        let mut main_builder = MeshBuilder::new(gfxtag!("menu_bg"));
        if main_bottom > 0. {
            main_builder.draw_filled_box(&Rectangle::new(0., 0., rect.w, main_bottom), bg_color);
        }
        let main_mesh = main_builder.alloc(&self.renderer).draw_untextured();

        let fade_mesh = if fade_distance > EPSILON && content_height > fade_top {
            let fade_bottom = rect.h.min(content_height);
            Some(shape::make_fade_mesh(&self.renderer, rect.w, fade_top, fade_bottom, bg_color))
        } else {
            None
        };

        (main_mesh, fade_mesh)
    }

    fn scrollview(&self, scroll: f32) {
        let item_height = self.get_item_height();
        let num_items = self.items.get_len() as f32;
        let content_height = num_items * item_height;

        let rect = self.rect.get();
        let max_scroll = (content_height - rect.h).max(0.);
        let scroll = scroll.clamp(0., max_scroll);
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
            let mut speed = self.speed.load(Ordering::Relaxed);
            if speed.abs() < EPSILON {
                break
            }

            while speed.abs() >= EPSILON {
                speed = self.speed.load(Ordering::Relaxed);
                let scroll = self.scroll.load(Ordering::Relaxed);
                self.scrollview(scroll + speed);
                self.redraw.trigger();
                speed *= resist;
                self.speed.store(speed, Ordering::Relaxed);
                darkfi::system::msleep(16).await;
            }

            self.speed.store(0., Ordering::Relaxed);
            break
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

        let atom = &mut self_.renderer.make_guard(gfxtag!("Menu::cancel_edit"));

        // Restore the saved items
        // It must exist otherwise theres a logic err
        let saved_items = self_.saved_items.lock().take().unwrap();
        self_.items.set_str_vec(atom, Role::App, saved_items).unwrap();

        // Exit edit mode
        self_.is_edit_mode.store(false, Ordering::Release);
        self_.invalidate_draw();
        self_.redraw.trigger();

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

        // Calculate deleted items by diffing saved and current items
        let saved_items = self_.saved_items.lock().take().unwrap();
        let current_items = self_.items.get_str_vec().unwrap();

        let deleted_items: Vec<String> =
            saved_items.into_iter().filter(|item| !current_items.contains(item)).collect();

        // Send the edit_done signal with deleted items
        let node = self_.node.upgrade().unwrap();
        let data = serialize(&deleted_items);
        node.trigger("edit_done", data).await.unwrap();

        self_.is_edit_mode.store(false, Ordering::Release);
        self_.invalidate_draw();
        self_.redraw.trigger();

        true
    }
}

#[async_trait]
impl UIObject for Menu {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = self.me.clone();
        let node_ref = &self.node.upgrade().unwrap();

        let me2 = me.clone();
        let cv = self.motion_cv.clone();
        let motion_task = ex.spawn(async move {
            loop {
                cv.wait().await;
                let Some(self_) = me2.upgrade() else {
                    t!("Menu destroyed before motion_task stopped");
                    break;
                };
                self_.handle_movement().await;
                cv.reset();
            }
        });

        let method_sub = node_ref.subscribe_method_call("cancel_edit").unwrap();
        let me2 = me.clone();
        let cancel_task =
            ex.spawn(async move { while Self::process_cancel_method(&me2, &method_sub).await {} });

        let method_sub = node_ref.subscribe_method_call("done_edit").unwrap();
        let me2 = me.clone();
        let done_task =
            ex.spawn(async move { while Self::process_done_method(&me2, &method_sub).await {} });

        let mut on_modify = OnModify::new(ex, self.node.clone(), me.clone());

        on_modify.when_change_external(self.items.clone(), |self_, _| async move {
            self_.invalidate_draw();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.rect.prop(), |self_, _| async move {
            self_.invalidate_draw();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.font_size.prop(), |self_, _| async move {
            self_.invalidate_draw();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.padding.clone(), |self_, _| async move {
            self_.invalidate_draw();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.text_color.prop(), |self_, _| async move {
            self_.invalidate_draw();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.bg_color.prop(), |self_, _| async move {
            self_.invalidate_draw();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.sep_size.prop(), |self_, _| async move {
            self_.invalidate_draw();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.sep_color.prop(), |self_, _| async move {
            self_.invalidate_draw();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.fade_zone.prop(), |self_, _| async move {
            self_.invalidate_draw();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.role1_color.prop(), |self_, _| async move {
            self_.invalidate_draw();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.role1_group.clone(), |self_, _| async move {
            self_.invalidate_draw();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.role2_color.prop(), |self_, _| async move {
            self_.invalidate_draw();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.role2_group.clone(), |self_, _| async move {
            self_.invalidate_draw();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.window_scale.prop(), |self_, _| async move {
            self_.invalidate_draw();
            self_.redraw.trigger();
        });

        let mut tasks = vec![motion_task, cancel_task, done_task];
        tasks.append(&mut on_modify.tasks);
        *self.tasks.lock() = tasks;
    }

    fn stop(&self) {
        *self.tasks.lock() = vec![];
        self.invalidate_draw();
    }

    async fn draw(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        *self.parent_rect.lock() = Some(parent_rect);

        // Rect property is its own memo: compare before/after eval.
        let prev_rect = self.rect.get();
        self.rect.eval(atom, &parent_rect).ok()?;
        let rect = self.rect.get();
        let rect_changed = rect != prev_rect;

        // The root shell is cheap: re-emit it every pass with the
        // current scroll so scroll pokes don't invalidate content.
        let scroll = self.scroll.load(Ordering::Relaxed);

        // Background meshes are viewport-anchored, so they only depend
        // on the rect and content height, never on scroll.
        let mut bg = self.bg_meshes.lock();
        if bg.is_none() || rect_changed {
            *bg = Some(self.make_bg_meshes(&rect));
        }
        let bg_meshes = bg.clone();
        drop(bg);

        // Content is reassembled when stale or when the scroll moved:
        // only fade-zone items and the separator mesh are rebuilt, the
        // rest reuse cached instructions.
        let mut cache = self.content_cache.lock();
        let stale = match cache.as_ref() {
            Some((cached_scroll, _)) => *cached_scroll != scroll,
            None => true,
        };
        if stale || rect_changed {
            *cache = Some((scroll, self.assemble_content(&rect, scroll)));
        }
        let instrs = cache.as_ref().unwrap().1.clone();
        drop(cache);

        let mut root_dcs = vec![];
        let mut draw_calls = vec![(
            self.root_dc_key,
            DrawCall {
                instrs: vec![
                    DrawInstruction::ApplyView(rect),
                    DrawInstruction::Move(Point::new(0., -scroll)),
                ],
                dcs: vec![],
                z_index: self.z_index.get(),
                debug_str: "menu_root",
            },
        )];

        if let Some((main_mesh, fade_mesh)) = bg_meshes {
            root_dcs.push(self.bg_dc_key);
            let mut bg_instrs = vec![
                DrawInstruction::Move(Point::new(0., scroll)),
                DrawInstruction::Draw(main_mesh),
            ];
            if let Some(fade_mesh) = fade_mesh {
                bg_instrs.push(DrawInstruction::Draw(fade_mesh));
            }
            draw_calls.push((
                self.bg_dc_key,
                DrawCall { instrs: bg_instrs, dcs: vec![], z_index: 0, debug_str: "menu_bg" },
            ));
        }

        root_dcs.push(self.content_dc_key);
        draw_calls[0].1.dcs = root_dcs;
        draw_calls.push((
            self.content_dc_key,
            DrawCall { instrs, dcs: vec![], z_index: 1, debug_str: "menu_content" },
        ));

        Some(DrawUpdate { key: self.root_dc_key, draw_calls })
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
            let hammy_half_size = font_size * 1.4;
            let hammy_center = rect.w - MENU_ICON_OFFSET - font_size * 0.56;
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
        let me = self.me.clone();
        let start_pos = mouse_pos;

        let ex = self.ex.clone();
        let long_press_task = ex.spawn(async move {
            darkfi::system::msleep(long_press_timeout() as u64).await;

            let Some(arc_self) = me.upgrade() else { return };
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
                    arc_self.invalidate_draw();
                    arc_self.redraw.trigger();
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
            self.invalidate_draw();
            self.redraw.trigger();
        }

        false
    }

    fn handle_touch_sync(&self, phase: TouchPhase, id: u64, touch_pos: Point) -> bool {
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
                    let hammy_half_size = font_size * 2.0;
                    let hammy_center = rect.w - MENU_ICON_OFFSET - font_size * 0.56;
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

                // Spawn a task to detect long press while the touch is
                // still held, mirroring the mouse path.
                let me = self.me.clone();
                let start_pos = touch_pos;
                let ex = self.ex.clone();
                let long_press_task = ex.spawn(async move {
                    darkfi::system::msleep(long_press_timeout() as u64).await;

                    let Some(arc_self) = me.upgrade() else { return };

                    let touch_info = arc_self.touch_info.lock().clone();
                    let Some(info) = touch_info else { return };

                    let movement_dist = ((info.last_pos.x - start_pos.x).powi(2) +
                        (info.last_pos.y - start_pos.y).powi(2))
                    .sqrt();

                    if movement_dist < LONG_PRESS_EPSILON &&
                        !arc_self.is_edit_mode.load(Ordering::Relaxed)
                    {
                        arc_self.save_items_layout();
                        arc_self.is_edit_mode.store(true, Ordering::Release);
                        let node = arc_self.node.upgrade().unwrap();
                        node.trigger("edit_active", vec![]).await.unwrap();
                        arc_self.invalidate_draw();
                        arc_self.redraw.trigger();
                    }
                });

                *self.long_press_task.lock() = Some(long_press_task);

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
                    self.invalidate_draw();
                    self.redraw.trigger();
                }

                let scroll = {
                    let mut touch_info = self.touch_info.lock();
                    let Some(info) = &mut *touch_info else { return false };

                    info.last_pos = touch_pos;
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
                self.redraw.trigger();
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
                // Cancel the long press detection task
                let task = self.long_press_task.lock().take();
                if let Some(task) = task {
                    task.cancel().await;
                }

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
