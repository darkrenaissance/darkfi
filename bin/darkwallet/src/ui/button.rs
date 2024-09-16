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

use async_trait::async_trait;
use miniquad::{MouseButton, TouchPhase};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Weak,
};

use crate::{
    gfx::{GraphicsEventPublisherPtr, Point, Rectangle},
    prop::{PropertyBool, PropertyPtr, PropertyRect, PropertyUint32, Role},
    pubsub::Subscription,
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
    ExecutorPtr,
};

use super::{DrawUpdate, UIObject};

pub type ButtonPtr = Arc<Button>;

pub struct Button {
    node_id: SceneNodeId,
    sg: SceneGraphPtr2,

    is_active: PropertyBool,
    rect: PropertyRect,
    z_index: PropertyUint32,

    mouse_btn_held: AtomicBool,
}

impl Button {
    pub async fn new(
        ex: ExecutorPtr,
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        event_pub: GraphicsEventPublisherPtr,
    ) -> Pimpl {
        let scene_graph = sg.lock().await;
        let node = scene_graph.get_node(node_id).unwrap();
        //let node_name = node.name.clone();
        let is_active = PropertyBool::wrap(node, Role::Internal, "is_active", 0).unwrap();
        let rect = PropertyRect::wrap(node, Role::Internal, "rect").unwrap();
        let z_index = PropertyUint32::wrap(node, Role::Internal, "z_index", 0).unwrap();
        //let sig = node.get_signal("click").expect("Button::click");
        drop(scene_graph);

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| Self {
            node_id,
            sg,
            is_active,
            rect,
            z_index,
            mouse_btn_held: AtomicBool::new(false),
        });

        Pimpl::Button(self_)
    }
}

#[async_trait]
impl UIObject for Button {
    fn z_index(&self) -> u32 {
        self.z_index.get()
    }

    async fn draw(&self, _: &SceneGraph, parent_rect: &Rectangle) -> Option<DrawUpdate> {
        let _ = self.rect.eval(parent_rect);
        None
    }

    async fn handle_mouse_btn_down(
        &self,
        sg: &SceneGraph,
        btn: MouseButton,
        mouse_pos: &Point,
    ) -> bool {
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

    async fn handle_mouse_btn_up(
        &self,
        sg: &SceneGraph,
        btn: MouseButton,
        mouse_pos: &Point,
    ) -> bool {
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

        debug!(target: "ui::button", "Mouse button clicked!");
        let node = sg.get_node(self.node_id).unwrap();
        node.trigger("click", vec![]).await.unwrap();

        true
    }

    async fn handle_touch(
        &self,
        sg: &SceneGraph,
        phase: TouchPhase,
        id: u64,
        touch_pos: &Point,
    ) -> bool {
        if !self.is_active.get() {
            return false
        }

        // Ignore multi-touch
        if id != 0 {
            return false
        }

        let rect = self.rect.get();
        if !rect.contains(touch_pos) {
            //debug!(target: "ui::chatview", "not inside rect");
            return false
        }

        // Simulate mouse events
        match phase {
            TouchPhase::Started => {
                self.handle_mouse_btn_down(sg, MouseButton::Left, touch_pos).await;
            }
            TouchPhase::Moved => {}
            TouchPhase::Ended => {
                self.handle_mouse_btn_up(sg, MouseButton::Left, touch_pos).await;
            }
            TouchPhase::Cancelled => {}
        }
        true
    }
}
