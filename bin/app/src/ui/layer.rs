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
use miniquad::{KeyCode, KeyMods, MouseButton, TouchPhase};
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::sync::Arc;
use tracing::instrument;

use crate::{
    gfx::{DrawCall, DrawInstruction, Point, Rectangle, RenderApi},
    prop::{BatchGuardPtr, PropertyAtomicGuard, PropertyBool, PropertyRect, PropertyUint32, Role},
    scene::{Pimpl, SceneNodePtr, SceneNodeWeak},
    util::i18n::I18nBabelFish,
    ExecutorPtr,
};

use super::{
    get_children_ordered, get_ui_object3, get_ui_object_ptr, DrawUpdate, OnModify, UIObject,
};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui:layer", $($arg)*); } }

pub type LayerPtr = Arc<Layer>;

pub struct Layer {
    node: SceneNodeWeak,
    render_api: RenderApi,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    dc_key: u64,

    is_visible: PropertyBool,
    rect: PropertyRect,
    z_index: PropertyUint32,
    priority: PropertyUint32,

    parent_rect: SyncMutex<Option<Rectangle>>,
}

impl Layer {
    pub async fn new(node: SceneNodeWeak, render_api: RenderApi) -> Pimpl {
        let node_ref = &node.upgrade().unwrap();
        t!("Layer::new({node_ref:?})");
        let is_visible = PropertyBool::wrap(node_ref, Role::Internal, "is_visible", 0).unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();

        let self_ = Arc::new(Self {
            node,
            render_api,
            tasks: SyncMutex::new(vec![]),
            dc_key: OsRng.gen(),

            is_visible,
            rect,
            z_index,
            priority,

            parent_rect: SyncMutex::new(None),
        });

        Pimpl::Layer(self_)
    }

    fn get_children(&self) -> Vec<SceneNodePtr> {
        let node = self.node.upgrade().unwrap();
        get_children_ordered(&node)
    }

    #[instrument(target = "ui::layer")]
    async fn redraw(self: Arc<Self>, batch: BatchGuardPtr) {
        let Some(parent_rect) = self.parent_rect.lock().clone() else { return };

        let atom = &mut batch.spawn();
        let Some(draw_update) = self.get_draw_calls(parent_rect, atom).await else {
            error!(target: "ui:layer", "Layer failed to draw");
            return
        };
        self.render_api.replace_draw_calls(batch.id, draw_update.draw_calls);
    }

    async fn get_draw_calls(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        self.rect.eval(atom, &parent_rect).ok()?;
        let rect = self.rect.get();

        // Apply viewport

        let mut draw_calls = vec![];
        let mut child_calls = vec![];

        // We should return a draw call so that if the layer is made visible, we can just
        // recalculate it and update in place.
        if self.is_visible.get() {
            for child in self.get_children() {
                let obj = get_ui_object3(&child);
                let Some(mut draw_update) = obj.draw(rect, atom).await else {
                    //t!("{child:?} draw returned none");
                    continue
                };

                draw_calls.append(&mut draw_update.draw_calls);
                child_calls.push(draw_update.key);
            }
        }

        let dc = DrawCall::new(
            vec![DrawInstruction::ApplyView(rect)],
            child_calls,
            self.z_index.get(),
            "layer",
        );
        draw_calls.push((self.dc_key, dc));
        Some(DrawUpdate { key: self.dc_key, draw_calls })
    }
}

#[async_trait]
impl UIObject for Layer {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    fn init(&self) {
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            obj.init();
        }
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);

        let mut on_modify = OnModify::new(ex.clone(), self.node.clone(), me.clone());
        on_modify.when_change(self.is_visible.prop(), Self::redraw);
        on_modify.when_change(self.rect.prop(), Self::redraw);
        on_modify.when_change(self.z_index.prop(), Self::redraw);

        *self.tasks.lock() = on_modify.tasks;

        for child in self.get_children() {
            let obj = get_ui_object_ptr(&child);
            obj.start(ex.clone()).await;
        }
    }

    fn stop(&self) {
        self.tasks.lock().clear();
        *self.parent_rect.lock() = None;
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            obj.stop();
        }
    }

    #[instrument(target = "ui::layer")]
    async fn draw(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        *self.parent_rect.lock() = Some(parent_rect);

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

        self.get_draw_calls(parent_rect, atom).await
    }

    async fn handle_char(&self, key: char, mods: KeyMods, repeat: bool) -> bool {
        if !self.is_visible.get() {
            return false
        }
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_char(key, mods, repeat).await {
                t!("handle_char({key:?}, {mods:?}, {repeat}) swallowed by {child:?}");
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
                t!("handle_key_down({key:?}, {mods:?}, {repeat}) swallowed by {child:?}");
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
                t!("handle_key_up({key:?}, {mods:?}) swallowed by {child:?}");
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
                t!("handle_mouse_btn_down({btn:?}, {mouse_pos:?}) swallowed by {child:?}");
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
                t!("handle_mouse_btn_up({btn:?}, {mouse_pos:?}) swallowed by {child:?}");
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
                t!("handle_mouse_move({mouse_pos:?}) swallowed by {child:?}");
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

    fn set_i18n(&self, i18n_fish: &I18nBabelFish) {
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            obj.set_i18n(i18n_fish);
        }
    }
}

// TODO: Drop

impl std::fmt::Debug for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.node.upgrade().unwrap())
    }
}
