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

use async_recursion::async_recursion;
use async_trait::async_trait;
use atomic_float::AtomicF32;
use miniquad::{KeyCode, KeyMods, MouseButton, TouchPhase};
use rand::{rngs::OsRng, Rng};
use std::sync::{atomic::Ordering, Arc, Mutex as SyncMutex, Weak};

use crate::{
    gfx::{GfxDrawCall, GfxDrawInstruction, Point, Rectangle, RenderApiPtr},
    prop::{PropertyBool, PropertyFloat32, PropertyPtr, PropertyRect, PropertyUint32, Role},
    scene::{Pimpl, SceneNodePtr, SceneNodeWeak},
    ExecutorPtr,
};

use super::{get_children_ordered, get_ui_object3, DrawUpdate, OnModify, UIObject};

pub type LayerPtr = Arc<Layer>;

pub struct Layer {
    node: SceneNodeWeak,
    render_api: RenderApiPtr,
    _tasks: Vec<smol::Task<()>>,
    dc_key: u64,

    is_visible: PropertyBool,
    rect: PropertyRect,
    z_index: PropertyUint32,

    parent_rect: SyncMutex<Option<Rectangle>>,
}

impl Layer {
    pub async fn new(node: SceneNodeWeak, render_api: RenderApiPtr, ex: ExecutorPtr) -> Pimpl {
        debug!(target: "ui::layer", "Layer::new()");

        let node_ref = &node.upgrade().unwrap();
        let is_visible = PropertyBool::wrap(node_ref, Role::Internal, "is_visible", 0).unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();

        let node_name = node_ref.name.clone();
        let node_id = node_ref.id;

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            let mut on_modify = OnModify::new(ex.clone(), node_name, node_id, me.clone());
            on_modify.when_change(is_visible.prop(), Self::redraw);
            on_modify.when_change(rect.prop(), Self::redraw);
            on_modify.when_change(z_index.prop(), Self::redraw);

            Self {
                node,
                render_api,
                _tasks: on_modify.tasks,
                dc_key: OsRng.gen(),

                is_visible,
                rect,
                z_index,

                parent_rect: SyncMutex::new(None),
            }
        });

        Pimpl::Layer(self_)
    }

    fn get_children(&self) -> Vec<SceneNodePtr> {
        let node = self.node.upgrade().unwrap();
        get_children_ordered(&node)
    }

    async fn redraw(self: Arc<Self>) {
        let Some(parent_rect) = self.parent_rect.lock().unwrap().clone() else { return };

        let Some(draw_update) = self.get_draw_calls(parent_rect).await else {
            error!(target: "ui::layer", "Layer failed to draw");
            return;
        };
        self.render_api.replace_draw_calls(draw_update.draw_calls);
        debug!(target: "ui::layer", "replace draw calls done");
    }

    async fn get_draw_calls(&self, parent_rect: Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::layer", "Layer::get_draw_calls()");
        self.rect.eval(&parent_rect).ok()?;
        let rect = self.rect.get();

        // Apply viewport

        let mut draw_calls = vec![];
        let mut child_calls = vec![];
        let mut freed_textures = vec![];
        let mut freed_buffers = vec![];

        // We should return a draw call so that if the layer is made visible, we can just
        // recalculate it and update in place.
        if self.is_visible.get() {
            for child in self.get_children() {
                let obj = get_ui_object3(&child);
                let Some(mut draw_update) = obj.draw(rect).await else {
                    debug!(target: "ui::layer", "Skipped draw() of {child:?}");
                    continue
                };

                draw_calls.append(&mut draw_update.draw_calls);
                child_calls.push(draw_update.key);
                freed_textures.append(&mut draw_update.freed_textures);
                freed_buffers.append(&mut draw_update.freed_buffers);
            }
        }

        let dc = GfxDrawCall {
            instrs: vec![GfxDrawInstruction::ApplyView(rect)],
            dcs: child_calls,
            z_index: self.z_index(),
        };
        draw_calls.push((self.dc_key, dc));
        Some(DrawUpdate { key: self.dc_key, draw_calls, freed_textures, freed_buffers })
    }
}

#[async_trait]
impl UIObject for Layer {
    fn z_index(&self) -> u32 {
        self.z_index.get()
    }

    async fn draw(&self, parent_rect: Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::layer", "Layer::draw()");
        *self.parent_rect.lock().unwrap() = Some(parent_rect);

        /*
        if !parent_rect.dim().contains(&offset_rect) {
            error!(
                target: "ui::layer",
                "layer rect {:?} is not inside parent {:?}",
                offset_rect, parent_rect
            );
            return None
        }
        */

        self.get_draw_calls(parent_rect).await
    }

    async fn handle_char(&self, key: char, mods: KeyMods, repeat: bool) -> bool {
        if !self.is_visible.get() {
            return false
        }
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_char(key, mods, repeat).await {
                return true
            }
        }
        false
    }

    async fn handle_key_down(&self, key: KeyCode, mods: KeyMods, repeat: bool) -> bool {
        if !self.is_visible.get() {
            return false
        }
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_key_down(key, mods, repeat).await {
                //debug!(target: "layer", "handle_key_down({key:?}, {mods:?}, {repeat}) swallowed by {child:?}");
                return true
            }
        }
        false
    }

    async fn handle_key_up(&self, key: KeyCode, mods: KeyMods) -> bool {
        if !self.is_visible.get() {
            return false
        }
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_key_up(key, mods).await {
                return true
            }
        }
        false
    }
    async fn handle_mouse_btn_down(&self, btn: MouseButton, mut mouse_pos: Point) -> bool {
        if !self.is_visible.get() {
            return false
        }
        mouse_pos -= self.rect.get().pos();
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_mouse_btn_down(btn, mouse_pos).await {
                return true
            }
        }
        false
    }
    async fn handle_mouse_btn_up(&self, btn: MouseButton, mut mouse_pos: Point) -> bool {
        if !self.is_visible.get() {
            return false
        }
        mouse_pos -= self.rect.get().pos();
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_mouse_btn_up(btn, mouse_pos).await {
                return true
            }
        }
        false
    }
    async fn handle_mouse_move(&self, mut mouse_pos: Point) -> bool {
        if !self.is_visible.get() {
            return false
        }
        mouse_pos -= self.rect.get().pos();
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_mouse_move(mouse_pos).await {
                return true
            }
        }
        false
    }
    async fn handle_mouse_wheel(&self, mut wheel_pos: Point) -> bool {
        if !self.is_visible.get() {
            return false
        }
        wheel_pos -= self.rect.get().pos();
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_mouse_wheel(wheel_pos).await {
                return true
            }
        }
        false
    }
    async fn handle_touch(&self, phase: TouchPhase, id: u64, mut touch_pos: Point) -> bool {
        if !self.is_visible.get() {
            return false
        }
        touch_pos -= self.rect.get().pos();
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_touch(phase, id, touch_pos).await {
                return true
            }
        }
        false
    }
    async fn handle_edit_text(&self, suggest_text: &str) -> bool {
        if !self.is_visible.get() {
            return false
        }
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_edit_text(suggest_text).await {
                //debug!(target: "layer", "handle_edit_text({suggest_text}) swallowed by {child:?}");
                return true
            }
        }
        false
    }
    async fn handle_commit_text(&self, suggest_text: &str) -> bool {
        if !self.is_visible.get() {
            return false
        }
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_commit_text(suggest_text).await {
                //debug!(target: "layer", "handle_edit_text({suggest_text}) swallowed by {child:?}");
                return true
            }
        }
        false
    }
}
