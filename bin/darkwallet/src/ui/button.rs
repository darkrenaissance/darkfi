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

use miniquad::{MouseButton, TouchPhase};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Weak,
};

use crate::{
    gfx::{GraphicsEventPublisherPtr, Point, Rectangle},
    prop::{PropertyBool, PropertyPtr, Role},
    pubsub::Subscription,
    scene::{Pimpl, SceneGraphPtr2, SceneNodeId},
    ExecutorPtr,
};

use super::{eval_rect, read_rect};

pub type ButtonPtr = Arc<Button>;

pub struct Button {
    node_id: SceneNodeId,
    #[allow(dead_code)]
    tasks: Vec<smol::Task<()>>,
    sg: SceneGraphPtr2,

    is_active: PropertyBool,
    rect: PropertyPtr,

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
        let rect = node.get_property("rect").expect("Button::rect");
        //let sig = node.get_signal("click").expect("Button::click");
        drop(scene_graph);

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            let ev_sub = event_pub.subscribe_mouse_btn_down();
            let me2 = me.clone();
            let mouse_btn_down_task =
                ex.spawn(async move { while Self::process_mouse_btn_down(&me2, &ev_sub).await {} });

            let ev_sub = event_pub.subscribe_mouse_btn_up();
            let me2 = me.clone();
            let mouse_btn_up_task =
                ex.spawn(async move { while Self::process_mouse_btn_up(&me2, &ev_sub).await {} });

            let ev_sub = event_pub.subscribe_touch();
            let me2 = me.clone();
            let touch_task =
                ex.spawn(async move { while Self::process_touch(&me2, &ev_sub).await {} });

            let tasks = vec![mouse_btn_down_task, mouse_btn_up_task, touch_task];

            Self { node_id, tasks, sg, is_active, rect, mouse_btn_held: AtomicBool::new(false) }
        });

        Pimpl::Button(self_)
    }

    async fn process_mouse_btn_down(
        me: &Weak<Self>,
        ev_sub: &Subscription<(MouseButton, f32, f32)>,
    ) -> bool {
        let Ok((btn, mouse_x, mouse_y)) = ev_sub.receive().await else {
            debug!(target: "ui::button", "Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before mouse_btn_down_task was stopped!");
        };

        if !self_.is_active.get() {
            return true
        }

        self_.handle_mouse_btn_down(btn, mouse_x, mouse_y);
        true
    }

    async fn process_mouse_btn_up(
        me: &Weak<Self>,
        ev_sub: &Subscription<(MouseButton, f32, f32)>,
    ) -> bool {
        let Ok((btn, mouse_x, mouse_y)) = ev_sub.receive().await else {
            debug!(target: "ui::button", "Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before mouse_btn_up_task was stopped!");
        };

        if !self_.is_active.get() {
            return true
        }

        self_.handle_mouse_btn_up(btn, mouse_x, mouse_y).await;
        true
    }

    async fn process_touch(
        me: &Weak<Self>,
        ev_sub: &Subscription<(TouchPhase, u64, f32, f32)>,
    ) -> bool {
        let Ok((phase, id, touch_x, touch_y)) = ev_sub.receive().await else {
            debug!(target: "ui::button", "Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before touch_task was stopped!");
        };

        if !self_.is_active.get() {
            return true
        }

        self_.handle_touch(phase, id, touch_x, touch_y).await;
        true
    }

    fn handle_mouse_btn_down(&self, btn: MouseButton, mouse_x: f32, mouse_y: f32) {
        if btn != MouseButton::Left {
            return
        }

        let mouse_pos = Point::from([mouse_x, mouse_y]);

        let Some(rect) = self.get_cached_rect() else { return };
        if !rect.contains(&mouse_pos) {
            return
        }

        self.mouse_btn_held.store(true, Ordering::Relaxed);
    }

    async fn handle_mouse_btn_up(&self, btn: MouseButton, mouse_x: f32, mouse_y: f32) {
        if btn != MouseButton::Left {
            return
        }

        // Did we start the click inside the button?
        let btn_held = self.mouse_btn_held.swap(false, Ordering::Relaxed);
        if !btn_held {
            return
        }

        let mouse_pos = Point::from([mouse_x, mouse_y]);

        // Are we releasing the click inside the button?
        let Some(rect) = self.get_cached_rect() else { return };
        if !rect.contains(&mouse_pos) {
            return
        }

        debug!(target: "ui::button", "Mouse button clicked!");
        let scene_graph = self.sg.lock().await;
        let node = scene_graph.get_node(self.node_id).unwrap();
        node.trigger("click", vec![]).await.unwrap();
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_x: f32, touch_y: f32) {
        // Ignore multi-touch
        if id != 0 {
            return
        }

        let Some(rect) = self.get_cached_rect() else { return };
        let touch_pos = Point { x: touch_x, y: touch_y };
        if !rect.contains(&touch_pos) {
            //debug!(target: "ui::chatview", "not inside rect");
            return
        }

        // Simulate mouse events
        match phase {
            TouchPhase::Started => self.handle_mouse_btn_down(MouseButton::Left, touch_x, touch_y),
            TouchPhase::Moved => {}
            TouchPhase::Ended => {
                self.handle_mouse_btn_up(MouseButton::Left, touch_x, touch_y).await
            }
            TouchPhase::Cancelled => {}
        }
    }

    fn get_cached_rect(&self) -> Option<Rectangle> {
        let Ok(rect) = read_rect(self.rect.clone()) else {
            error!(target: "ui::button", "cached_rect is None");
            return None
        };
        Some(rect)
    }

    pub fn set_parent_rect(&self, parent_rect: &Rectangle) {
        if let Err(err) = eval_rect(self.rect.clone(), parent_rect) {
            panic!("Button bad rect property: {}", err);
        }
    }
}
