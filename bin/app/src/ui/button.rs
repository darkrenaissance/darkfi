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
use miniquad::{MouseButton, TouchPhase};
use rand::{rngs::OsRng, Rng};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tracing::instrument;

use crate::{
    gfx::{gfxtag, DrawCall, DrawInstruction, DrawMesh, Point, Rectangle, RenderApi, Renderer},
    mesh::MeshBuilder,
    prop::{PropertyAtomicGuard, PropertyBool, PropertyRect, PropertyUint32, Role},
    scene::{Pimpl, SceneNodeWeak},
};

use super::{DrawUpdate, UIObject};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::button", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::button", $($arg)*); } }

pub type ButtonPtr = Arc<Button>;

pub struct Button {
    node: SceneNodeWeak,
    renderer: Renderer,

    is_active: PropertyBool,
    rect: PropertyRect,
    priority: PropertyUint32,
    z_index: PropertyUint32,
    debug: PropertyBool,
    dc_key: u64,

    mouse_btn_held: AtomicBool,
}

impl Button {
    pub async fn new(node: SceneNodeWeak, renderer: Renderer) -> Pimpl {
        let node_ref = &node.upgrade().unwrap();
        let is_active = PropertyBool::wrap(node_ref, Role::Internal, "is_active", 0).unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let debug = PropertyBool::wrap(node_ref, Role::Internal, "debug", 0).unwrap();

        let self_ = Arc::new(Self {
            node,
            renderer,
            is_active,
            rect,
            priority,
            z_index,
            debug,
            dc_key: OsRng.gen(),
            mouse_btn_held: AtomicBool::new(false),
        });

        Pimpl::Button(self_)
    }
}

#[async_trait]
impl UIObject for Button {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    #[instrument(target = "ui::button")]
    async fn draw(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        if let Err(e) = self.rect.eval(atom, &parent_rect) {
            warn!(target: "ui::button", "Rect eval failure: {e}");
        }

        if !self.debug.get() {
            return None;
        }

        let rect = self.rect.get();
        let mut mesh = MeshBuilder::new(gfxtag!("button_debug"));
        mesh.draw_outline(&rect, [1., 0., 0., 1.], 1.);

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                DrawCall::new(
                    vec![DrawInstruction::Draw(mesh.alloc(&self.renderer).draw_untextured())],
                    vec![],
                    self.z_index.get(),
                    "button_debug",
                ),
            )],
        })
    }

    async fn handle_mouse_btn_down(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if !self.is_active.get() {
            return false
        }

        if btn != MouseButton::Left {
            return false
        }

        let rect = self.rect.get();
        if !rect.contains(mouse_pos) {
            return false
        }

        self.mouse_btn_held.store(true, Ordering::Relaxed);
        true
    }

    async fn handle_mouse_btn_up(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        t!("handle_mouse_btn_up({btn:?}, {mouse_pos:?})");
        if !self.is_active.get() {
            return false
        }

        if btn != MouseButton::Left {
            return false
        }

        // Did we start the click inside the button?
        let btn_held = self.mouse_btn_held.swap(false, Ordering::Relaxed);
        if !btn_held {
            return false
        }

        // Are we releasing the click inside the button?
        let rect = self.rect.get();
        if !rect.contains(mouse_pos) {
            return false
        }

        d!("Button clicked!");
        let node = self.node.upgrade().unwrap();
        node.trigger("click", vec![]).await.unwrap();

        true
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_pos: Point) -> bool {
        //t!("handle_touch({phase:?}, {id}, {touch_pos:?})");
        if !self.is_active.get() {
            return false
        }

        // Ignore multi-touch
        if id != 0 {
            return false
        }

        let rect = self.rect.get();
        if !rect.contains(touch_pos) {
            //t!("not inside rect");
            return false
        }

        // Simulate mouse events
        match phase {
            TouchPhase::Started => self.handle_mouse_btn_down(MouseButton::Left, touch_pos).await,
            TouchPhase::Moved => false,
            TouchPhase::Ended => self.handle_mouse_btn_up(MouseButton::Left, touch_pos).await,
            TouchPhase::Cancelled => false,
        }
    }
}

impl std::fmt::Debug for Button {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.node.upgrade().unwrap())
    }
}
