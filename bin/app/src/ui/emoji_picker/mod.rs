/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
use darkfi_serial::Encodable;
use image::ImageReader;
use miniquad::{MouseButton, TouchPhase};
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::{
    io::Cursor,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Weak,
    },
};

use crate::{
    gfx::{
        GfxDrawCall, GfxDrawInstruction, GfxDrawMesh, GfxTextureId, ManagedTexturePtr, Point,
        Rectangle, RenderApi,
    },
    mesh::{MeshBuilder, MeshInfo, COLOR_WHITE},
    prop::{
        PropertyAtomicGuard, PropertyFloat32, PropertyPtr, PropertyRect, PropertyStr,
        PropertyUint32, Role,
    },
    scene::{Pimpl, SceneNodePtr, SceneNodeWeak},
    text::{self, GlyphPositionIter, TextShaper, TextShaperPtr},
    util::unixtime,
    ExecutorPtr,
};

use super::{DrawUpdate, OnModify, UIObject};

mod default;
mod emoji;
pub use emoji::{EmojiMeshes, EmojiMeshesPtr};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::emoji_picker", $($arg)*) } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::emoji_picker", $($arg)*) } }

struct TouchInfo {
    start_pos: Point,
    start_scroll: f32,
    is_scroll: bool,
}

pub type EmojiPickerPtr = Arc<EmojiPicker>;

pub struct EmojiPicker {
    node: SceneNodeWeak,
    render_api: RenderApi,
    tasks: SyncMutex<Vec<smol::Task<()>>>,

    dc_key: u64,
    emoji_meshes: EmojiMeshesPtr,

    rect: PropertyRect,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    scroll: PropertyFloat32,
    emoji_size: PropertyFloat32,
    mouse_scroll_speed: PropertyFloat32,

    window_scale: PropertyFloat32,
    parent_rect: SyncMutex<Option<Rectangle>>,
    is_mouse_hover: AtomicBool,
    touch_info: SyncMutex<Option<TouchInfo>>,
}

impl EmojiPicker {
    pub async fn new(
        node: SceneNodeWeak,
        window_scale: PropertyFloat32,
        render_api: RenderApi,
        emoji_meshes: EmojiMeshesPtr,
        ex: ExecutorPtr,
    ) -> Pimpl {
        t!("EmojiPicker::new()");

        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let scroll = PropertyFloat32::wrap(node_ref, Role::Internal, "scroll", 0).unwrap();
        let emoji_size = PropertyFloat32::wrap(node_ref, Role::Internal, "emoji_size", 0).unwrap();
        let mouse_scroll_speed =
            PropertyFloat32::wrap(node_ref, Role::Internal, "mouse_scroll_speed", 0).unwrap();

        let node_name = node_ref.name.clone();
        let node_id = node_ref.id;

        let self_ = Arc::new(Self {
            node,
            render_api,
            tasks: SyncMutex::new(vec![]),

            dc_key: OsRng.gen(),
            emoji_meshes,

            rect,
            z_index,
            priority,
            scroll,
            emoji_size,
            mouse_scroll_speed,

            window_scale,
            parent_rect: SyncMutex::new(None),
            is_mouse_hover: AtomicBool::new(false),
            touch_info: SyncMutex::new(None),
        });

        Pimpl::EmojiPicker(self_)
    }

    fn emojis_per_line(&self) -> f32 {
        let emoji_size = self.emoji_size.get();
        let rect_w = self.rect.get().w;
        //d!("rect_w = {rect_w}");
        (rect_w / emoji_size).floor()
    }
    fn calc_off_x(&self) -> f32 {
        let emoji_size = self.emoji_size.get();
        let rect_w = self.rect.get().w;
        let n = self.emojis_per_line();
        let off_x = (rect_w - emoji_size) / (n - 1.);
        off_x
    }

    fn max_scroll(&self) -> f32 {
        let emojis_len = self.emoji_meshes.lock().get_list().len() as f32;
        let emoji_size = self.emoji_size.get();
        let cols = self.emojis_per_line();
        let rows = (emojis_len / cols).ceil();

        let rect_h = self.rect.get().h;
        let height = rows * emoji_size;
        if height < rect_h {
            return 0.
        }
        height - rect_h
    }

    async fn click_emoji(&self, pos: Point) {
        let n_cols = self.emojis_per_line();
        let emoji_size = self.emoji_size.get();
        let scroll = self.scroll.get();

        // Emojis have spacing along the x axis.
        // If the screen width is 2000, and emoji_size is 30, then that's 66 emojis.
        // But that's 66.66px per emoji.
        let real_width = self.rect.get().w / n_cols;
        //d!("click_emoji({pos:?})");
        let col = (pos.x / real_width).floor();

        let y = pos.y + scroll;
        let row = (y / emoji_size).floor();
        //d!("emoji_size = {emoji_size}, col = {col}, row = {row}");

        //d!("idx = col + row * n_cols = {col} + {row} * {n_cols}");
        let idx = (col + row * n_cols).round() as usize;
        //d!("    = {idx}, emoji_len = {}", emoji::EMOJI_LIST.len());

        let emoji_selected = {
            let mut emoji_meshes = self.emoji_meshes.lock();
            let emoji_list = emoji_meshes.get_list();

            if idx < emoji_list.len() {
                let emoji = emoji_list[idx].clone();
                Some(emoji)
            } else {
                None
            }
        };
        match emoji_selected {
            Some(emoji) => {
                d!("Selected emoji: {emoji}");
                let mut param_data = vec![];
                emoji.encode(&mut param_data).unwrap();
                let node = self.node.upgrade().unwrap();
                node.trigger("emoji_select", param_data).await.unwrap();
            }
            None => d!("Index out of bounds: {idx}"),
        }
    }

    fn redraw(&self) {
        let atom = &mut PropertyAtomicGuard::new();
        let trace_id = rand::random();
        let timest = unixtime();
        t!("redraw({:?}) [timest={timest}, trace_id={trace_id}]", self.node.upgrade().unwrap());
        let Some(parent_rect) = self.parent_rect.lock().clone() else { return };

        let Some(draw_update) = self.get_draw_calls(parent_rect, trace_id, atom) else {
            error!(target: "ui::emoji_picker", "Emoji picker failed to draw");
            return
        };
        self.render_api.replace_draw_calls(timest, draw_update.draw_calls);
        t!("redraw DONE [trace_id={trace_id}]");
    }

    fn get_draw_calls(
        &self,
        parent_rect: Rectangle,
        trace_id: u32,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        if let Err(e) = self.rect.eval(&parent_rect) {
            warn!(target: "ui::emoji_picker", "Rect eval failed: {e}");
            return None
        }

        // Clamp scroll if needed due to window size change
        let max_scroll = self.max_scroll();
        if self.scroll.get() > max_scroll {
            self.scroll.set(atom, max_scroll);
        }

        let rect = self.rect.get();
        let mut instrs = vec![GfxDrawInstruction::ApplyView(rect)];

        let off_x = self.calc_off_x();
        let emoji_size = self.emoji_size.get();

        let mut emoji_meshes = self.emoji_meshes.lock();
        let emoji_list_len = emoji_meshes.get_list().len();

        let mut x = emoji_size / 2.;
        let mut y = emoji_size / 2. - self.scroll.get();
        for i in 0..emoji_list_len {
            let pos = Point::new(x, y);
            let mesh = emoji_meshes.get(i);
            instrs.extend_from_slice(&[
                GfxDrawInstruction::SetPos(pos),
                GfxDrawInstruction::Draw(mesh),
            ]);

            x += off_x;
            if x > rect.w {
                x = emoji_size / 2.;
                y += emoji_size;
                //d!("Line break after idx={i}");
            }

            if y > rect.h + emoji_size {
                break
            }
        }

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                GfxDrawCall { instrs, dcs: vec![], z_index: self.z_index.get() },
            )],
        })
    }
}

#[async_trait]
impl UIObject for EmojiPicker {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);

        async fn redraw(self_: Arc<EmojiPicker>) {
            self_.redraw();
        }

        let mut on_modify = OnModify::new(ex, self.node.clone(), me.clone());
        on_modify.when_change(self.rect.prop(), redraw);
        on_modify.when_change(self.z_index.prop(), redraw);

        *self.tasks.lock() = on_modify.tasks;
    }

    fn stop(&self) {
        self.tasks.lock().clear();
        self.emoji_meshes.lock().clear();
    }

    async fn draw(
        &self,
        parent_rect: Rectangle,
        trace_id: u32,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        t!("EmojiPicker::draw({parent_rect:?}, {trace_id})");
        *self.parent_rect.lock() = Some(parent_rect);
        self.get_draw_calls(parent_rect, trace_id, atom)
    }

    async fn handle_mouse_move(&self, mut mouse_pos: Point) -> bool {
        let rect = self.rect.get();
        self.is_mouse_hover.store(rect.contains(mouse_pos), Ordering::Relaxed);
        false
    }

    async fn handle_mouse_wheel(&self, wheel_pos: Point) -> bool {
        if !self.is_mouse_hover.load(Ordering::Relaxed) {
            return false
        }
        t!("handle_mouse_wheel()");
        let atom = &mut PropertyAtomicGuard::new();

        let mut scroll = self.scroll.get();
        scroll -= self.mouse_scroll_speed.get() * wheel_pos.y;
        scroll = scroll.clamp(0., self.max_scroll());
        self.scroll.set(atom, scroll);

        self.redraw();

        true
    }

    async fn handle_mouse_btn_up(&self, btn: MouseButton, mut mouse_pos: Point) -> bool {
        let rect = self.rect.get();
        if !rect.contains(mouse_pos) {
            return false
        }
        mouse_pos.x -= rect.x;
        mouse_pos.y -= rect.y;
        self.click_emoji(mouse_pos).await;

        true
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, mut touch_pos: Point) -> bool {
        // Ignore multi-touch
        if id != 0 {
            return false
        }

        let atom = &mut PropertyAtomicGuard::new();

        let rect = self.rect.get();
        let pos = touch_pos - Point::new(rect.x, rect.y);

        // We need this cos you cannot hold mutex and call async fn
        // todo: clean this up
        let mut emoji_is_clicked = false;
        {
            let mut touch_info = self.touch_info.lock();
            match phase {
                TouchPhase::Started => {
                    if !rect.contains(touch_pos) {
                        return false
                    }

                    *touch_info = Some(TouchInfo {
                        start_pos: pos,
                        start_scroll: self.scroll.get(),
                        is_scroll: false,
                    });
                }
                TouchPhase::Moved => {
                    if let Some(touch_info) = touch_info.as_mut() {
                        let y_diff = touch_info.start_pos.y - pos.y;
                        if y_diff.abs() > 0.5 {
                            touch_info.is_scroll = true;
                        }

                        if touch_info.is_scroll {
                            let mut scroll = touch_info.start_scroll + y_diff;
                            scroll = scroll.clamp(0., self.max_scroll());
                            self.scroll.set(atom, scroll);
                            self.redraw();
                        }
                    } else {
                        return false
                    }
                }
                TouchPhase::Ended | TouchPhase::Cancelled => {
                    if let Some(touch_info) = &*touch_info {
                        if !touch_info.is_scroll {
                            emoji_is_clicked = true;
                        }
                    } else {
                        return false
                    }
                    *touch_info = None;
                }
            }
        }
        if emoji_is_clicked {
            self.click_emoji(pos).await;
        }

        true
    }
}

impl Drop for EmojiPicker {
    fn drop(&mut self) {
        self.render_api.replace_draw_calls(unixtime(), vec![(self.dc_key, Default::default())]);
    }
}
