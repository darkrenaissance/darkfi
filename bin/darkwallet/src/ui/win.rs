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

use miniquad::{KeyCode, KeyMods, MouseButton, TouchPhase};
use std::sync::{Arc, Weak};

use crate::{
    gfx::{GfxDrawCall, GraphicsEventPublisherPtr, Rectangle, RenderApiPtr},
    prop::{PropertyPtr, Role},
    pubsub::Subscription,
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
    ExecutorPtr,
};

use super::{get_child_nodes_ordered, get_ui_object, OnModify, Stoppable};

pub type WindowPtr = Arc<Window>;

pub struct Window {
    node_id: SceneNodeId,
    sg: SceneGraphPtr2,

    // Task is dropped at the end of the scope for Window, hence ending it
    #[allow(dead_code)]
    tasks: Vec<smol::Task<()>>,
    screen_size_prop: PropertyPtr,
    render_api: RenderApiPtr,
}

impl Window {
    pub async fn new(
        ex: ExecutorPtr,
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        render_api: RenderApiPtr,
        event_pub: GraphicsEventPublisherPtr,
    ) -> Pimpl {
        debug!(target: "ui::win", "Window::new()");

        let scene_graph = sg.lock().await;
        let node = scene_graph.get_node(node_id).unwrap();
        let node_name = node.name.clone();
        let screen_size_prop = node.get_property("screen_size").unwrap();
        let scale_prop = node.get_property("scale").unwrap();
        drop(scene_graph);

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            // Start a task monitoring for window resize events
            // which updates screen_size
            let ev_sub = event_pub.subscribe_resize();
            let screen_size_prop2 = screen_size_prop.clone();
            let me2 = me.clone();
            let sg2 = sg.clone();
            let resize_task = ex.spawn(async move {
                loop {
                    let Ok((w, h)) = ev_sub.receive().await else {
                        debug!(target: "ui::win", "Event relayer closed");
                        break
                    };

                    debug!(target: "ui::win", "Window resized ({w}, {h})");
                    // Now update the properties
                    screen_size_prop2.set_f32(Role::Internal, 0, w).unwrap();
                    screen_size_prop2.set_f32(Role::Internal, 1, h).unwrap();

                    let Some(self_) = me2.upgrade() else {
                        // Should not happen
                        panic!("self destroyed before modify_task was stopped!");
                    };

                    let sg = sg2.lock().await;
                    self_.draw(&sg).await;
                }
            });

            let ev_sub = event_pub.subscribe_char();
            let me2 = me.clone();
            let char_task =
                ex.spawn(async move { while Self::process_char(&me2, &ev_sub).await {} });

            let ev_sub = event_pub.subscribe_key_down();
            let me2 = me.clone();
            let key_down_task =
                ex.spawn(async move { while Self::process_key_down(&me2, &ev_sub).await {} });

            let ev_sub = event_pub.subscribe_key_up();
            let me2 = me.clone();
            let key_up_task =
                ex.spawn(async move { while Self::process_key_up(&me2, &ev_sub).await {} });

            let ev_sub = event_pub.subscribe_mouse_btn_down();
            let me2 = me.clone();
            let mouse_btn_down_task =
                ex.spawn(async move { while Self::process_mouse_btn_down(&me2, &ev_sub).await {} });

            let ev_sub = event_pub.subscribe_mouse_btn_up();
            let me2 = me.clone();
            let mouse_btn_up_task =
                ex.spawn(async move { while Self::process_mouse_btn_up(&me2, &ev_sub).await {} });

            let ev_sub = event_pub.subscribe_mouse_move();
            let me2 = me.clone();
            let mouse_move_task =
                ex.spawn(async move { while Self::process_mouse_move(&me2, &ev_sub).await {} });

            let ev_sub = event_pub.subscribe_mouse_wheel();
            let me2 = me.clone();
            let mouse_wheel_task =
                ex.spawn(async move { while Self::process_mouse_wheel(&me2, &ev_sub).await {} });

            let ev_sub = event_pub.subscribe_touch();
            let me2 = me.clone();
            let touch_task =
                ex.spawn(async move { while Self::process_touch(&me2, &ev_sub).await {} });

            let sg2 = sg.clone();
            let redraw_fn = move |self_: Arc<Self>| {
                let sg = sg2.clone();
                async move {
                    let sg = sg.lock().await;
                    self_.draw(&sg).await;
                }
            };

            let mut on_modify = OnModify::new(ex.clone(), node_name, node_id, me.clone());
            on_modify.when_change(scale_prop, redraw_fn);

            let mut tasks = vec![
                resize_task,
                char_task,
                key_down_task,
                key_up_task,
                mouse_btn_down_task,
                mouse_btn_up_task,
                mouse_move_task,
                mouse_wheel_task,
                touch_task,
            ];
            tasks.append(&mut on_modify.tasks);

            Self { node_id, sg, tasks, screen_size_prop, render_api }
        });

        Pimpl::Window(self_)
    }

    async fn process_char(me: &Weak<Self>, ev_sub: &Subscription<(char, KeyMods, bool)>) -> bool {
        let Ok((key, mods, repeat)) = ev_sub.receive().await else {
            debug!(target: "ui::win", "Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before char_task was stopped!");
        };

        self_.handle_char(key, mods, repeat).await;
        true
    }

    async fn process_key_down(
        me: &Weak<Self>,
        ev_sub: &Subscription<(KeyCode, KeyMods, bool)>,
    ) -> bool {
        let Ok((key, mods, repeat)) = ev_sub.receive().await else {
            debug!(target: "ui::win", "Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before char_task was stopped!");
        };

        self_.handle_key_down(key, mods, repeat).await;
        true
    }

    async fn process_key_up(me: &Weak<Self>, ev_sub: &Subscription<(KeyCode, KeyMods)>) -> bool {
        let Ok((key, mods)) = ev_sub.receive().await else {
            debug!(target: "ui::win", "Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before char_task was stopped!");
        };

        self_.handle_key_up(key, mods).await;
        true
    }

    async fn process_mouse_btn_down(
        me: &Weak<Self>,
        ev_sub: &Subscription<(MouseButton, f32, f32)>,
    ) -> bool {
        let Ok((btn, mouse_x, mouse_y)) = ev_sub.receive().await else {
            debug!(target: "ui::editbox", "Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before mouse_btn_down_task was stopped!");
        };

        self_.handle_mouse_btn_down(btn, mouse_x, mouse_y).await;
        true
    }

    async fn process_mouse_btn_up(
        me: &Weak<Self>,
        ev_sub: &Subscription<(MouseButton, f32, f32)>,
    ) -> bool {
        let Ok((btn, mouse_x, mouse_y)) = ev_sub.receive().await else {
            debug!(target: "ui::editbox", "Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before mouse_btn_up_task was stopped!");
        };

        self_.handle_mouse_btn_up(btn, mouse_x, mouse_y).await;
        true
    }

    async fn process_mouse_move(me: &Weak<Self>, ev_sub: &Subscription<(f32, f32)>) -> bool {
        let Ok((mouse_x, mouse_y)) = ev_sub.receive().await else {
            debug!(target: "ui::editbox", "Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before mouse_move_task was stopped!");
        };

        self_.handle_mouse_move(mouse_x, mouse_y).await;
        true
    }

    async fn process_mouse_wheel(me: &Weak<Self>, ev_sub: &Subscription<(f32, f32)>) -> bool {
        let Ok((wheel_x, wheel_y)) = ev_sub.receive().await else {
            debug!(target: "ui::chatview", "Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before mouse_wheel_task was stopped!");
        };

        self_.handle_mouse_wheel(wheel_x, wheel_y).await;
        true
    }

    async fn process_touch(
        me: &Weak<Self>,
        ev_sub: &Subscription<(TouchPhase, u64, f32, f32)>,
    ) -> bool {
        let Ok((phase, id, touch_x, touch_y)) = ev_sub.receive().await else {
            debug!(target: "ui::editbox", "Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before touch_task was stopped!");
        };

        self_.handle_touch(phase, id, touch_x, touch_y).await;
        true
    }

    async fn handle_char(&self, key: char, mods: KeyMods, repeat: bool) {
        let sg = self.sg.lock().await;

        for child_id in get_child_nodes_ordered(&sg, self.node_id) {
            let node = sg.get_node(child_id).unwrap();
            let obj = get_ui_object(node);
            if obj.handle_char(&sg, key, mods, repeat).await {
                return
            }
        }
    }

    async fn handle_key_down(&self, key: KeyCode, mods: KeyMods, repeat: bool) {
        let sg = self.sg.lock().await;

        for child_id in get_child_nodes_ordered(&sg, self.node_id) {
            let node = sg.get_node(child_id).unwrap();
            let obj = get_ui_object(node);
            if obj.handle_key_down(&sg, key, mods, repeat).await {
                return
            }
        }
    }

    async fn handle_key_up(&self, key: KeyCode, mods: KeyMods) {
        let sg = self.sg.lock().await;

        for child_id in get_child_nodes_ordered(&sg, self.node_id) {
            let node = sg.get_node(child_id).unwrap();
            let obj = get_ui_object(node);
            if obj.handle_key_up(&sg, key, mods).await {
                return
            }
        }
    }

    async fn handle_mouse_btn_down(&self, btn: MouseButton, mouse_x: f32, mouse_y: f32) {
        let sg = self.sg.lock().await;

        for child_id in get_child_nodes_ordered(&sg, self.node_id) {
            let node = sg.get_node(child_id).unwrap();
            let obj = get_ui_object(node);
            if obj.handle_mouse_btn_down(&sg, btn.clone(), mouse_x, mouse_y).await {
                return
            }
        }
    }

    async fn handle_mouse_btn_up(&self, btn: MouseButton, mouse_x: f32, mouse_y: f32) {
        let sg = self.sg.lock().await;

        for child_id in get_child_nodes_ordered(&sg, self.node_id) {
            let node = sg.get_node(child_id).unwrap();
            let obj = get_ui_object(node);
            if obj.handle_mouse_btn_up(&sg, btn.clone(), mouse_x, mouse_y).await {
                return
            }
        }
    }

    async fn handle_mouse_move(&self, mouse_x: f32, mouse_y: f32) {
        let sg = self.sg.lock().await;

        for child_id in get_child_nodes_ordered(&sg, self.node_id) {
            let node = sg.get_node(child_id).unwrap();
            let obj = get_ui_object(node);
            if obj.handle_mouse_move(&sg, mouse_x, mouse_y).await {
                return
            }
        }
    }

    async fn handle_mouse_wheel(&self, wheel_x: f32, wheel_y: f32) {
        let sg = self.sg.lock().await;

        for child_id in get_child_nodes_ordered(&sg, self.node_id) {
            let node = sg.get_node(child_id).unwrap();
            let obj = get_ui_object(node);
            if obj.handle_mouse_wheel(&sg, wheel_x, wheel_y).await {
                return
            }
        }
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_x: f32, touch_y: f32) {
        let sg = self.sg.lock().await;

        for child_id in get_child_nodes_ordered(&sg, self.node_id) {
            let node = sg.get_node(child_id).unwrap();
            let obj = get_ui_object(node);
            if obj.handle_touch(&sg, phase, id, touch_x, touch_y).await {
                return
            }
        }
    }

    pub async fn draw(&self, sg: &SceneGraph) {
        let screen_width = self.screen_size_prop.get_f32(0).unwrap();
        let screen_height = self.screen_size_prop.get_f32(1).unwrap();
        debug!(target: "ui::win", "Window::draw({screen_width}, {screen_height})");

        // SceneGraph should remain locked for the entire draw
        let self_node = sg.get_node(self.node_id).unwrap();

        let parent_rect = Rectangle::from_array([0., 0., screen_width, screen_height]);

        let mut draw_calls = vec![];
        let mut child_calls = vec![];
        let mut freed_textures = vec![];
        let mut freed_buffers = vec![];

        for child_inf in self_node.get_children2() {
            let node = sg.get_node(child_inf.id).unwrap();
            //debug!(target: "ui::win", "Window::draw() calling draw() for node '{}':{}", node.name, node.id);

            let dcs = match &node.pimpl {
                Pimpl::RenderLayer(layer) => layer.draw(sg, &parent_rect).await,
                _ => {
                    error!(target: "ui::win", "unhandled pimpl type");
                    continue
                }
            };
            let Some(mut draw_update) = dcs else { continue };
            draw_calls.append(&mut draw_update.draw_calls);
            child_calls.push(draw_update.key);
            freed_textures.append(&mut draw_update.freed_textures);
            freed_buffers.append(&mut draw_update.freed_buffers);
        }

        let root_dc = GfxDrawCall { instrs: vec![], dcs: child_calls, z_index: 0 };
        draw_calls.push((0, root_dc));
        //debug!(target: "ui::win", "  => {:?}", draw_calls);

        self.render_api.replace_draw_calls(draw_calls);

        for texture in freed_textures {
            self.render_api.delete_texture(texture);
        }
        for buff in freed_buffers {
            self.render_api.delete_buffer(buff);
        }

        debug!(target: "ui::win", "Window::draw() - replaced draw call");
    }
}

// Nodes should be stopped before being removed
impl Stoppable for Window {
    async fn stop(&self) {}
}
