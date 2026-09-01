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
use miniquad::{KeyCode, KeyMods, MouseButton};
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::sync::Arc;
use tracing::instrument;

use crate::{
    gfx::{gfxtag, DrawCall, DrawInstruction, Point, Rectangle, Renderer},
    mesh::MeshBuilder,
    prop::{
        PropertyAtomicGuard, PropertyBool, PropertyFloat32, PropertyRect, PropertyUint32, Role,
    },
    scene::{Pimpl, SceneNodePtr, SceneNodeWeak},
    util::i18n::I18nBabelFish,
    ExecutorPtr,
};

use super::{
    gesture, get_children_ordered, get_ui_object3, get_ui_object_ptr, DrawUpdate, GestureTarget,
    OnModify, RedrawTrigger, UIObject,
};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui:layer", $($arg)*); } }

pub type LayerPtr = Arc<Layer>;

pub struct Layer {
    node: SceneNodeWeak,
    renderer: Renderer,
    redraw: RedrawTrigger,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    dc_key: u64,

    is_visible: PropertyBool,
    rect: PropertyRect,
    alpha: PropertyFloat32,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    debug: PropertyBool,
}

impl Layer {
    pub async fn new(_node: SceneNodeWeak, renderer: Renderer, redraw: RedrawTrigger) -> Pimpl {
        let node_ref = &_node.upgrade().unwrap();
        let is_visible = PropertyBool::wrap(node_ref, Role::Internal, "is_visible", 0).unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let alpha = PropertyFloat32::wrap(node_ref, Role::Internal, "alpha", 0).unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let debug = PropertyBool::wrap(node_ref, Role::Internal, "debug", 0).unwrap();

        let self_ = Arc::new(Self {
            node: _node,
            renderer,
            redraw,
            tasks: SyncMutex::new(vec![]),
            dc_key: OsRng.gen(),

            is_visible,
            rect,
            alpha,
            z_index,
            priority,
            debug,
        });

        Pimpl::Layer(self_)
    }

    fn get_children(&self) -> Vec<SceneNodePtr> {
        let node = self.node.upgrade().unwrap();
        get_children_ordered(&node)
    }

    async fn get_draw_calls(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        self.rect.eval(atom, &parent_rect).ok()?;
        let rect = self.rect.get();

        let alpha = self.alpha.get();

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

        let mut instrs = vec![DrawInstruction::ApplyView(rect)];

        if self.debug.get() {
            let mut mesh = MeshBuilder::new(gfxtag!("layer_debug"));
            mesh.draw_outline(&Rectangle::new(0., 0., rect.w, rect.h), [1., 0., 0., 1.], 1.);
            instrs.push(DrawInstruction::Draw(mesh.alloc(&self.renderer).draw_untextured()));
        }

        instrs.push(DrawInstruction::SetAlpha(alpha));

        let dc = DrawCall::new(instrs, child_calls, self.z_index.get(), "layer");
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
        // Stateless in the pass: property changes only request a draw pass.
        // All layer output is recomputed by the pass itself. Internal-role
        // sets are eval echoes of the pass, so only external (App) changes
        // trigger — otherwise every pass would queue another, forever.
        on_modify.when_change_external(self.is_visible.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.rect.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.alpha.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.z_index.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.debug.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });

        *self.tasks.lock() = on_modify.tasks;

        for child in self.get_children() {
            let obj = get_ui_object_ptr(&child);
            obj.start(ex.clone()).await;
        }
    }

    fn stop(&self) {
        self.tasks.lock().clear();
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
    fn gesture_hit_test(&self, pos: Point) -> bool {
        if !self.is_visible.get() {
            return false
        }

        let local = pos - self.rect.get().pos();
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.gesture_hit_test(local) {
                return true
            }
        }

        false
    }

    fn gesture_descend(&self, pos: Point, offset: Point, chain: &mut Vec<GestureTarget>) {
        if !self.is_visible.get() {
            return
        }

        let rect_pos = self.rect.get().pos();
        let local = pos - rect_pos;
        let children: Vec<_> =
            self.get_children().iter().map(|child| get_ui_object_ptr(child)).collect();
        gesture::scan_children(&children, local, offset + rect_pos, chain);
    }

    async fn handle_gesture(&self, gesture: gesture::GestureAction) -> bool {
        if !self.is_visible.get() {
            return false
        }

        let mut gesture = gesture;
        let rect_pos = self.rect.get().pos();
        gesture.translate(crate::gfx::Vector { x: -rect_pos.x, y: -rect_pos.y });

        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_gesture(gesture.clone()).await {
                t!("handle_gesture swallowed by {child:?}");
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
