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
use miniquad::{MouseButton, TouchPhase};
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use crate::{
    gfx::{gfxtag, DrawCall, DrawInstruction, Point, Rectangle, RenderApi},
    mesh::MeshBuilder,
    prop::{
        BatchGuardPtr, PropertyAtomicGuard, PropertyBool, PropertyColor, PropertyFloat32,
        PropertyPtr, PropertyRect, PropertyUint32, Role,
    },
    scene::{Pimpl, SceneNodeWeak},
    text, ExecutorPtr,
};

use super::{DrawUpdate, OnModify, UIObject};

const EPSILON: f32 = 0.001;
const BIG_EPSILON: f32 = 0.05;

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::menu", $($arg)*); } }

#[derive(Clone)]
struct TouchInfo {
    start_scroll: f32,
    start_y: f32,
    start_instant: std::time::Instant,
    samples: VecDeque<(std::time::Instant, f32)>,
    last_instant: std::time::Instant,
    last_y: f32,
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
    render_api: RenderApi,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    dc_key: u64,

    is_visible: PropertyBool,
    rect: PropertyRect,
    scroll: PropertyFloat32,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    items: PropertyPtr,

    font_size: PropertyFloat32,
    padding: PropertyPtr,
    text_color: PropertyColor,
    bg_color: PropertyColor,
    sep_size: PropertyFloat32,
    sep_color: PropertyColor,
    window_scale: PropertyFloat32,

    mouse_pos: SyncMutex<Point>,
    touch_info: SyncMutex<Option<TouchInfo>>,
    touch_is_active: AtomicBool,
    scroll_start_accel: PropertyFloat32,
    scroll_resist: PropertyFloat32,
    motion_cv: Arc<CondVar>,
    speed: AtomicF32,

    parent_rect: SyncMutex<Option<Rectangle>>,
}

impl Menu {
    pub async fn new(
        node: SceneNodeWeak,
        window_scale: PropertyFloat32,
        render_api: RenderApi,
    ) -> Pimpl {
        let node_ref = &node.upgrade().unwrap();
        let is_visible = PropertyBool::wrap(node_ref, Role::Internal, "is_visible", 0).unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let scroll = PropertyFloat32::wrap(node_ref, Role::Internal, "scroll", 0).unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let items = node_ref.get_property("items").expect("Menu::items");

        let font_size = PropertyFloat32::wrap(node_ref, Role::Internal, "font_size", 0).unwrap();
        let padding = node_ref.get_property("padding").expect("Menu::padding");
        let text_color = PropertyColor::wrap(node_ref, Role::Internal, "text_color").unwrap();
        let bg_color = PropertyColor::wrap(node_ref, Role::Internal, "bg_color").unwrap();
        let sep_size = PropertyFloat32::wrap(node_ref, Role::Internal, "sep_size", 0).unwrap();
        let sep_color = PropertyColor::wrap(node_ref, Role::Internal, "sep_color").unwrap();

        let scroll_start_accel =
            PropertyFloat32::wrap(node_ref, Role::Internal, "scroll_start_accel", 0).unwrap();
        let scroll_resist =
            PropertyFloat32::wrap(node_ref, Role::Internal, "scroll_resist", 0).unwrap();

        let motion_cv = Arc::new(CondVar::new());

        let self_ = Arc::new(Self {
            node: node.clone(),
            render_api: render_api.clone(),
            tasks: SyncMutex::new(vec![]),
            dc_key: OsRng.gen(),
            is_visible,
            rect,
            scroll,
            z_index,
            priority,
            items,
            font_size,
            padding,
            text_color,
            bg_color,
            sep_size,
            sep_color,
            window_scale,
            mouse_pos: SyncMutex::new(Point::new(0., 0.)),
            touch_info: SyncMutex::new(None),
            touch_is_active: AtomicBool::new(false),
            scroll_start_accel,
            scroll_resist,
            motion_cv,
            speed: AtomicF32::new(0.),
            parent_rect: SyncMutex::new(None),
        });

        Pimpl::Menu(self_)
    }

    fn get_item_height(&self) -> f32 {
        self.font_size.get() + self.padding.get_f32(1).unwrap() * 2.0
    }

    fn get_selected_item_index(&self, click_y: f32) -> Option<usize> {
        let rect = self.rect.get();
        let scroll = self.scroll.get();
        let item_height = self.get_item_height();

        let content_y = click_y - rect.y - scroll;

        if content_y >= 0. && content_y < self.items.get_len() as f32 * item_height {
            Some((content_y / item_height) as usize)
        } else {
            None
        }
    }

    async fn handle_selection(&self, item_idx: usize) {
        if item_idx < self.items.get_len() {
            let item_name = self.items.get_str(item_idx).unwrap();
            let node = self.node.upgrade().unwrap();
            node.trigger("selected", item_name.as_bytes().to_vec()).await.unwrap();
        }
    }

    fn get_draw_calls(
        &self,
        atom: &mut PropertyAtomicGuard,
        parent_rect: Rectangle,
    ) -> Option<DrawUpdate> {
        self.rect.eval(atom, &parent_rect).ok()?;
        let rect = self.rect.get();

        let mut instrs = vec![DrawInstruction::ApplyView(rect)];

        let scroll = self.scroll.get();
        let item_height = self.get_item_height();
        let font_size = self.font_size.get();
        let padding_x = self.padding.get_f32(0).unwrap();
        let padding_y = self.padding.get_f32(1).unwrap();
        let text_color = self.text_color.get();
        let bg_color = self.bg_color.get();
        let sep_size = self.sep_size.get();
        let sep_color = self.sep_color.get();
        let window_scale = self.window_scale.get();

        let num_items = self.items.get_len();
        //d!("Number of menu items: {num_items}");

        let start_idx = (scroll / item_height).floor() as usize;
        let end_idx = ((scroll + rect.h) / item_height).ceil() as usize;
        let end_idx = end_idx.min(num_items);

        for idx in start_idx..end_idx {
            let item_text = self.items.get_str(idx).unwrap();
            let y_pos = idx as f32 * item_height - scroll;

            // Draw background
            instrs.push(DrawInstruction::SetPos(Point::new(0., y_pos)));

            let mut mesh = MeshBuilder::new(gfxtag!("menu_bg"));
            mesh.draw_filled_box(&Rectangle::new(0., 0., rect.w, item_height), bg_color);
            let bg_mesh = mesh.alloc(&self.render_api).draw_untextured();

            instrs.push(DrawInstruction::Draw(bg_mesh));

            // Draw text
            let layout = text::make_layout(
                &item_text,
                text_color,
                font_size,
                1.0,
                window_scale,
                Some(rect.w - padding_x * 2.),
                &[],
            );
            //d!("Menu item: {item_text}");

            let text_instr = text::render_layout(&layout, &self.render_api, gfxtag!("menu_text"));

            instrs.push(DrawInstruction::SetPos(Point::new(padding_x, y_pos + padding_y)));
            instrs.extend(text_instr);

            // Draw separator (except for last item)
            if idx < num_items - 1 && sep_size > 0. {
                let sep_y = y_pos + item_height - sep_size;

                instrs.push(DrawInstruction::SetPos(Point::new(0., sep_y)));

                let mut mesh = MeshBuilder::new(gfxtag!("menu_sep"));
                mesh.draw_filled_box(&Rectangle::new(0., 0., rect.w, sep_size), sep_color);
                let sep_mesh = mesh.alloc(&self.render_api).draw_untextured();

                instrs.push(DrawInstruction::Draw(sep_mesh));
            }
        }

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                DrawCall::new(instrs, vec![], self.z_index.get(), "menu"),
            )],
        })
    }

    async fn redraw(self: Arc<Self>, batch: BatchGuardPtr) {
        let Some(parent_rect) = self.parent_rect.lock().clone() else { return };

        let atom = &mut batch.spawn();
        let Some(draw_update) = self.get_draw_calls(atom, parent_rect) else { return };

        self.render_api.replace_draw_calls(atom.batch_id, draw_update.draw_calls);
    }

    async fn scrollview(&self, scroll: f32, atom: &mut PropertyAtomicGuard) {
        let item_height = self.get_item_height();
        let num_items = self.items.get_len() as f32;
        let content_height = num_items * item_height;

        let rect = self.rect.get();
        let max_scroll = (content_height - rect.h).max(0.);

        let scroll = scroll.clamp(0., max_scroll);
        self.scroll.set(atom, scroll);

        if let Some(parent_rect) = self.parent_rect.lock().clone() {
            if let Some(draw_update) = self.get_draw_calls(atom, parent_rect) {
                self.render_api.replace_draw_calls(atom.batch_id, draw_update.draw_calls);
            }
        }
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
                let atom = &mut self.render_api.make_guard(gfxtag!("Menu::movement"));

                let scroll = self.scroll.get();
                self.scrollview(scroll + speed, atom).await;
                speed *= resist;
                self.speed.store(speed, Ordering::Relaxed);
                darkfi::system::msleep(16).await;
            }

            self.speed.store(0., Ordering::Relaxed);
        }
    }

    fn end_touch_phase(&self, touch_y: f32) {
        let touch_info = self.touch_info.lock();
        let Some(info) = &*touch_info else { return };

        if let Some((dt, _)) = info.first_sample() {
            if dt > EPSILON {
                let velocity = (touch_y - info.start_y) / dt;
                self.start_scroll(-velocity);
            }
        }
    }
}

#[async_trait]
impl UIObject for Menu {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);

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

        let mut on_modify = OnModify::new(ex, self.node.clone(), me.clone());

        on_modify.when_change(self.items.clone(), Self::redraw);
        on_modify.when_change(self.scroll.prop(), Self::redraw);
        on_modify.when_change(self.rect.prop(), Self::redraw);
        on_modify.when_change(self.font_size.prop(), Self::redraw);
        on_modify.when_change(self.padding.clone(), Self::redraw);
        on_modify.when_change(self.text_color.prop(), Self::redraw);
        on_modify.when_change(self.bg_color.prop(), Self::redraw);
        on_modify.when_change(self.sep_size.prop(), Self::redraw);
        on_modify.when_change(self.sep_color.prop(), Self::redraw);

        let mut tasks = vec![motion_task];
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

        let click_y = mouse_pos.y - rect.y;
        if let Some(item_idx) = self.get_selected_item_index(click_y) {
            self.handle_selection(item_idx).await;
            true
        } else {
            false
        }
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
        false
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_pos: Point) -> bool {
        if id != 0 {
            return false
        }

        let rect = self.rect.get();

        match phase {
            TouchPhase::Started => {
                if !rect.contains(touch_pos) {
                    *self.touch_info.lock() = None;
                    return false
                }

                self.touch_is_active.store(true, Ordering::Relaxed);
                *self.touch_info.lock() = Some(TouchInfo::new(self.scroll.get(), touch_pos.y));
                true
            }

            TouchPhase::Moved => {
                let (_needs_update, scroll) = {
                    let mut touch_info = self.touch_info.lock();
                    let Some(info) = &mut *touch_info else { return false };

                    info.last_y = touch_pos.y;
                    info.push_sample(touch_pos.y);

                    let last_elapsed = info.last_instant.elapsed().as_micros();
                    if last_elapsed <= 20_000 {
                        return true
                    }
                    info.last_instant = std::time::Instant::now();

                    let dist = touch_pos.y - info.start_y;
                    if dist.abs() < BIG_EPSILON {
                        return true
                    }

                    (true, info.start_scroll - dist)
                };

                let atom = &mut self.render_api.make_guard(gfxtag!("Menu::touch"));
                self.scrollview(scroll, atom).await;
                true
            }

            TouchPhase::Ended | TouchPhase::Cancelled => {
                let is_tap = {
                    let touch_info = self.touch_info.lock();
                    let Some(info) = &*touch_info else { return true };

                    (touch_pos.y - info.start_y).abs() < BIG_EPSILON
                };

                if is_tap {
                    let click_y = touch_pos.y - rect.y;

                    if let Some(item_idx) = self.get_selected_item_index(click_y) {
                        self.handle_selection(item_idx).await;
                    }
                }

                self.end_touch_phase(touch_pos.y);
                true
            }
        }
    }
}
