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
use darkfi_serial::Encodable;
use miniquad::{MouseButton, TouchPhase};
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::{
    gfx::{
        gfxtag, Dimension, DrawCall, DrawInstruction, EpochCache, Point, Rectangle, RenderApi,
        Renderer,
    },
    prop::{PropertyAtomicGuard, PropertyFloat32, PropertyPtr, PropertyRect, PropertyUint32, Role},
    scene::{Pimpl, SceneNodeWeak},
    ExecutorPtr,
};

use super::{DrawUpdate, OnModify, RedrawTrigger, UIObject};

mod default;
use default::DEFAULT_EMOJI_LIST;
mod emoji;
pub use emoji::{EmojiMeshes, EmojiMeshesPtr};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::emoji_picker", $($arg)*) } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::emoji_picker", $($arg)*) } }

#[derive(Clone)]
struct TouchInfo {
    start_pos: Point,
    start_scroll: f32,
    is_scroll: bool,
}

pub type EmojiPickerPtr = Arc<EmojiPicker>;

pub struct EmojiPicker {
    node: SceneNodeWeak,
    renderer: Renderer,
    tasks: SyncMutex<Vec<smol::Task<()>>>,

    dc_key: u64,
    emoji_meshes: EmojiMeshesPtr,

    rect: PropertyRect,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    scroll: PropertyFloat32,
    emoji_size: PropertyFloat32,
    /// `[x, y]` padding around each emoji icon
    emoji_margin: PropertyPtr,
    mouse_scroll_speed: PropertyFloat32,

    redraw: RedrawTrigger,
    /// Cached emoji grid instructions. Empty means stale (rect, scroll or
    /// z_index changed). Scroll is set with an internal role, so scroll
    /// mutation sites invalidate explicitly. Entries from a dead UI epoch
    /// are evicted automatically.
    draw_cache: EpochCache<Vec<DrawInstruction>>,
    is_mouse_hover: AtomicBool,
    touch_info: SyncMutex<Option<TouchInfo>>,
}

impl EmojiPicker {
    pub async fn new(
        node: SceneNodeWeak,
        renderer: Renderer,
        emoji_meshes: EmojiMeshesPtr,
        redraw: RedrawTrigger,
    ) -> Pimpl {
        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let scroll = PropertyFloat32::wrap(node_ref, Role::Internal, "scroll", 0).unwrap();
        let emoji_size = PropertyFloat32::wrap(node_ref, Role::Internal, "emoji_size", 0).unwrap();
        let emoji_margin = node_ref.get_property("emoji_margin").unwrap();
        let mouse_scroll_speed =
            PropertyFloat32::wrap(node_ref, Role::Internal, "mouse_scroll_speed", 0).unwrap();

        let draw_cache = EpochCache::new(&renderer);

        let self_ = Arc::new(Self {
            node,
            renderer,
            tasks: SyncMutex::new(vec![]),

            dc_key: OsRng.gen(),
            emoji_meshes,

            rect,
            z_index,
            priority,
            scroll,
            emoji_size,
            emoji_margin,
            mouse_scroll_speed,

            redraw,
            draw_cache,
            is_mouse_hover: AtomicBool::new(false),
            touch_info: SyncMutex::new(None),
        });

        Pimpl::EmojiPicker(self_)
    }

    /// Size of a grid cell, i.e. the emoji icon plus its surrounding margin
    fn cell(&self) -> Dimension {
        Dimension {
            w: self.emoji_size.get() + self.emoji_margin.get_f32(0).unwrap(),
            h: self.emoji_size.get() + self.emoji_margin.get_f32(1).unwrap(),
        }
    }

    /// Number of emoji cells that fit in a row (at least 1)
    fn emojis_per_line(&self) -> usize {
        let cell = self.cell();
        let rect_w = self.rect.get().w;
        ((rect_w / cell.w).floor() as usize).max(1)
    }

    /// Horizontal pitch between cells. The row is spread evenly across the
    /// full width, so leftover space is distributed into the gaps.
    fn calc_off_x(&self) -> f32 {
        let cell = self.cell();
        let rect_w = self.rect.get().w;
        let n = self.emojis_per_line();
        if n <= 1 {
            return 0.
        }
        (rect_w - cell.w) / (n as f32 - 1.)
    }

    fn max_scroll(&self) -> f32 {
        let emojis_len = DEFAULT_EMOJI_LIST.len() as f32;
        let cell = self.cell();
        let cols = self.emojis_per_line() as f32;
        let rows = (emojis_len / cols).ceil();

        let rect_h = self.rect.get().h;
        let height = rows * cell.h;
        if height < rect_h {
            return 0.
        }
        height - rect_h
    }

    async fn click_emoji(&self, pos: Point) {
        let n_cols = self.emojis_per_line();
        let cell = self.cell();
        let off_x = self.calc_off_x();
        let scroll = self.scroll.get();

        // Icons are spread with pitch `off_x` and width `cell.w`. The gap
        // between two neighboring cells is `off_x - cell.w`, and the
        // boundary between them sits in the middle of that gap.
        let col = if off_x > 0. {
            let gap = off_x - cell.w;
            let shifted_x = pos.x - gap / 2.;
            (shifted_x / off_x).floor()
        } else {
            0.
        };

        let y = pos.y + scroll;
        let row = (y / cell.h).floor();

        let idx = (col + row * n_cols as f32).round() as usize;

        let emoji_selected = {
            if idx < DEFAULT_EMOJI_LIST.len() {
                let emoji = DEFAULT_EMOJI_LIST[idx].to_string();
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

    fn get_draw_calls(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        // Rect property is its own memo: compare before/after eval.
        let prev_rect = self.rect.get();
        if let Err(e) = self.rect.eval(atom, &parent_rect) {
            warn!(target: "ui:emoji_picker", "Rect eval failed: {e}");
            return None
        }
        let rect = self.rect.get();
        let rect_changed = rect != prev_rect;

        // Clamp scroll if needed due to window size change
        let max_scroll = self.max_scroll();
        if self.scroll.get() > max_scroll {
            self.scroll.set(atom, max_scroll);
            self.draw_cache.clear();
        }

        // The grid depends on rect and scroll. Compute under the cache
        // lock so concurrent invalidations land before or after, never
        // between.
        if rect_changed {
            self.draw_cache.clear();
        }
        if !self.emoji_meshes.clone().start_make() {
            // Skip the draw while the atlas is unbuilt so an empty grid
            // never lands in the cache; the pass retries once built.
            return None
        }
        let instrs = self.draw_cache.get_or_insert_with(|| {
            let mut instrs = vec![DrawInstruction::ApplyView(rect)];

            let off_x = self.calc_off_x();
            let cell = self.cell();
            let n_cols = self.emojis_per_line();
            let scroll = self.scroll.get();

            for i in 0..DEFAULT_EMOJI_LIST.len() {
                let col = (i % n_cols) as f32;
                let row = (i / n_cols) as f32;
                let x = col * off_x;
                let y = row * cell.h - scroll;
                if y > rect.h + cell.h {
                    break
                }

                let Some((mesh, ink)) = self.emoji_meshes.get(i) else { break };
                // Center the emoji's ink inside its cell so the margin pads
                // it evenly on all sides. The ink origin sits above the
                // mesh origin (text baseline), hence the -ink.x/-ink.y.
                let pos = Point::new(
                    x + (cell.w - ink.w) / 2. - ink.x,
                    y + (cell.h - ink.h) / 2. - ink.y,
                );
                instrs.extend_from_slice(&[
                    DrawInstruction::SetPos(pos),
                    DrawInstruction::Draw(mesh),
                ]);
            }

            instrs
        });

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                DrawCall::new(instrs, vec![], self.z_index.get(), "emoji"),
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

        let mut on_modify = OnModify::new(ex, self.node.clone(), me.clone());
        // Invalidate the cache, then request a pass. Internal-role echoes
        // (the pass's own evals) are skipped.
        on_modify.when_change_external(self.rect.prop(), |self_, _| async move {
            self_.draw_cache.clear();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.z_index.prop(), |self_, _| async move {
            self_.draw_cache.clear();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.emoji_size.prop(), |self_, _| async move {
            let emoji_size = self_.emoji_size.get();
            self_.emoji_meshes.set_size(emoji_size);
            self_.draw_cache.clear();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.emoji_margin.clone(), |self_, _| async move {
            self_.draw_cache.clear();
            self_.redraw.trigger();
        });

        *self.tasks.lock() = on_modify.tasks;
    }

    fn stop(&self) {
        self.tasks.lock().clear();
        self.draw_cache.clear();
        self.emoji_meshes.clear();
    }

    #[instrument(target = "ui::emoji_picker")]
    async fn draw(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        self.get_draw_calls(parent_rect, atom)
    }
    async fn handle_mouse_move(&self, mouse_pos: Point) -> bool {
        let rect = self.rect.get();
        self.is_mouse_hover.store(rect.contains(mouse_pos), Ordering::Relaxed);
        false
    }

    async fn handle_mouse_wheel(&self, wheel_pos: Point) -> bool {
        if !self.is_mouse_hover.load(Ordering::Relaxed) {
            return false
        }
        t!("handle_mouse_wheel()");
        let atom = &mut self.redraw.make_guard(gfxtag!("EmojiPicker::handle_mouse_wheel"));

        let mut scroll = self.scroll.get();
        scroll -= self.mouse_scroll_speed.get() * wheel_pos.y;
        scroll = scroll.clamp(0., self.max_scroll());
        self.scroll.set(atom, scroll);

        self.draw_cache.clear();

        true
    }

    async fn handle_mouse_btn_up(&self, _btn: MouseButton, mut mouse_pos: Point) -> bool {
        let rect = self.rect.get();
        if !rect.contains(mouse_pos) {
            return false
        }
        mouse_pos.x -= rect.x;
        mouse_pos.y -= rect.y;
        self.click_emoji(mouse_pos).await;

        true
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_pos: Point) -> bool {
        // Ignore multi-touch
        if id != 0 {
            return false
        }

        let atom = &mut self.redraw.make_guard(gfxtag!("EmojiPicker::handle_touch"));

        let rect = self.rect.get();
        let pos = touch_pos - Point::new(rect.x, rect.y);

        // We need this cos you cannot hold mutex and call async fn
        // todo: clean this up
        let mut emoji_is_clicked = false;
        {
            match phase {
                TouchPhase::Started => {
                    let mut touch_info = self.touch_info.lock();
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
                    let (touch_info, y_diff) = {
                        let mut touch_info = self.touch_info.lock();
                        let Some(touch_info) = touch_info.as_mut() else {
                            return false;
                        };

                        let y_diff = touch_info.start_pos.y - pos.y;
                        if y_diff.abs() > 0.5 {
                            touch_info.is_scroll = true;
                        }
                        (touch_info.clone(), y_diff)
                    };

                    if touch_info.is_scroll {
                        let mut scroll = touch_info.start_scroll + y_diff;
                        scroll = scroll.clamp(0., self.max_scroll());
                        self.scroll.set(atom, scroll);

                        self.draw_cache.clear();
                    }
                }
                TouchPhase::Ended | TouchPhase::Cancelled => {
                    let touch_info = std::mem::take(&mut *self.touch_info.lock());
                    let Some(touch_info) = touch_info else { return false };
                    if !touch_info.is_scroll {
                        emoji_is_clicked = true;
                    }
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
        self.renderer.replace_draw_calls(vec![(self.dc_key, Default::default())]);
    }
}

impl std::fmt::Debug for EmojiPicker {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.node.upgrade().unwrap())
    }
}
