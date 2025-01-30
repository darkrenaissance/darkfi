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

use miniquad::{KeyCode, KeyMods, MouseButton, TouchPhase};
use std::sync::{Arc, OnceLock, Weak};

use crate::{
    gfx::{
        GfxDrawCall, GfxDrawInstruction, GraphicsEventPublisherPtr, Point, Rectangle, RenderApi,
    },
    prop::{PropertyAtomicGuard, PropertyDimension, PropertyFloat32, PropertyPtr, Role},
    pubsub::Subscription,
    scene::{Pimpl, SceneNodePtr, SceneNodeWeak},
    util::unixtime,
    ExecutorPtr,
};

use super::{get_children_ordered, get_ui_object3, get_ui_object_ptr, OnModify};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::window", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::window", $($arg)*); } }

#[cfg(feature = "emulate-android")]
const EMULATE_TOUCH: bool = true;

#[cfg(not(feature = "emulate-android"))]
const EMULATE_TOUCH: bool = false;

pub type WindowPtr = Arc<Window>;

pub struct Window {
    node: SceneNodeWeak,

    tasks: OnceLock<Vec<smol::Task<()>>>,
    screen_size: PropertyDimension,
    scale: PropertyFloat32,
    render_api: RenderApi,
}

impl Window {
    pub async fn new(node: SceneNodeWeak, render_api: RenderApi) -> Pimpl {
        t!("Window::new()");

        let node_ref = &node.upgrade().unwrap();
        let screen_size = PropertyDimension::wrap(node_ref, Role::Internal, "screen_size").unwrap();
        let scale = PropertyFloat32::wrap(node_ref, Role::Internal, "scale", 0).unwrap();

        let self_ = Arc::new(Self { node, tasks: OnceLock::new(), screen_size, scale, render_api });

        Pimpl::Window(self_)
    }

    pub async fn start(self: Arc<Self>, event_pub: GraphicsEventPublisherPtr, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);

        // Start a task monitoring for window resize events
        // which updates screen_size
        let ev_sub = event_pub.subscribe_resize();
        let screen_size2 = self.screen_size.clone();
        let me2 = me.clone();
        let resize_task = ex.spawn(async move {
            loop {
                let Ok(size) = ev_sub.receive().await else {
                    t!("Event relayer closed");
                    break
                };

                d!("Window resized {size:?}");
                let atom = &mut PropertyAtomicGuard::new();

                // Now update the properties
                screen_size2.set(atom, size);

                let Some(self_) = me2.upgrade() else {
                    // Should not happen
                    panic!("self destroyed before modify_task was stopped!");
                };

                self_.draw().await;
            }
        });

        let ev_sub = event_pub.subscribe_char();
        let me2 = me.clone();
        let char_task = ex.spawn(async move { while Self::process_char(&me2, &ev_sub).await {} });

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
        let touch_task = ex.spawn(async move { while Self::process_touch(&me2, &ev_sub).await {} });

        let redraw_fn = move |self_: Arc<Self>| async move {
            self_.draw().await;
        };

        let mut on_modify = OnModify::new(ex.clone(), self.node.clone(), me.clone());
        on_modify.when_change(self.scale.prop(), redraw_fn);

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

        #[cfg(target_os = "android")]
        {
            let (sender, recvr) = async_channel::unbounded();
            crate::android::set_sender(sender);
            let me2 = me.clone();
            let autosuggest_task = ex.spawn(async move {
                loop {
                    let Ok(ev) = recvr.recv().await else {
                        t!("Event relayer closed");
                        break
                    };

                    let Some(self_) = me2.upgrade() else {
                        // Should not happen
                        panic!("self destroyed before modify_task was stopped!");
                    };

                    self_.handle_autosuggest(ev).await;
                }
            });
            tasks.push(autosuggest_task);
        }

        self.tasks.set(tasks);

        for child in self.get_children() {
            let obj = get_ui_object_ptr(&child);
            obj.start(ex.clone()).await;
        }
    }

    async fn process_char(me: &Weak<Self>, ev_sub: &Subscription<(char, KeyMods, bool)>) -> bool {
        let Ok((key, mods, repeat)) = ev_sub.receive().await else {
            t!("Event relayer closed");
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
            t!("Event relayer closed");
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
            t!("Event relayer closed");
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
        ev_sub: &Subscription<(MouseButton, Point)>,
    ) -> bool {
        let Ok((btn, mouse_pos)) = ev_sub.receive().await else {
            t!("Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before mouse_btn_down_task was stopped!");
        };

        self_.handle_mouse_btn_down(btn, mouse_pos).await;
        true
    }

    async fn process_mouse_btn_up(
        me: &Weak<Self>,
        ev_sub: &Subscription<(MouseButton, Point)>,
    ) -> bool {
        let Ok((btn, mouse_pos)) = ev_sub.receive().await else {
            t!("Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before mouse_btn_up_task was stopped!");
        };

        self_.handle_mouse_btn_up(btn, mouse_pos).await;
        true
    }

    async fn process_mouse_move(me: &Weak<Self>, ev_sub: &Subscription<Point>) -> bool {
        let Ok(mouse_pos) = ev_sub.receive().await else {
            t!("Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before mouse_move_task was stopped!");
        };

        self_.handle_mouse_move(mouse_pos).await;
        true
    }

    async fn process_mouse_wheel(me: &Weak<Self>, ev_sub: &Subscription<Point>) -> bool {
        let Ok(wheel_pos) = ev_sub.receive().await else {
            t!("Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before mouse_wheel_task was stopped!");
        };

        self_.handle_mouse_wheel(wheel_pos).await;
        true
    }

    async fn process_touch(
        me: &Weak<Self>,
        ev_sub: &Subscription<(TouchPhase, u64, Point)>,
    ) -> bool {
        let Ok((phase, id, touch_pos)) = ev_sub.receive().await else {
            t!("Event relayer closed");
            return false
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before touch_task was stopped!");
        };

        self_.handle_touch(phase, id, touch_pos).await;
        true
    }

    fn get_children(&self) -> Vec<SceneNodePtr> {
        let node = self.node.upgrade().unwrap();
        get_children_ordered(&node)
    }

    async fn handle_char(&self, key: char, mods: KeyMods, repeat: bool) {
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_char(key, mods, repeat).await {
                return
            }
        }
    }

    async fn handle_key_down(&self, key: KeyCode, mods: KeyMods, repeat: bool) {
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_key_down(key, mods, repeat).await {
                return
            }
        }
    }

    async fn handle_key_up(&self, key: KeyCode, mods: KeyMods) {
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_key_up(key, mods).await {
                return
            }
        }
    }

    /// Converts from screen to local coords
    fn local_scale(&self, point: &mut Point) {
        point.x /= self.scale.get();
        point.y /= self.scale.get();
    }

    async fn handle_mouse_btn_down(&self, btn: MouseButton, mut mouse_pos: Point) {
        self.local_scale(&mut mouse_pos);
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if EMULATE_TOUCH {
                if obj.handle_touch(TouchPhase::Started, 0, mouse_pos).await {
                    return
                }
            } else {
                if obj.handle_mouse_btn_down(btn.clone(), mouse_pos).await {
                    return
                }
            }
        }
    }

    async fn handle_mouse_btn_up(&self, btn: MouseButton, mut mouse_pos: Point) {
        self.local_scale(&mut mouse_pos);
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if EMULATE_TOUCH {
                if obj.handle_touch(TouchPhase::Ended, 0, mouse_pos).await {
                    return
                }
            } else {
                if obj.handle_mouse_btn_up(btn.clone(), mouse_pos).await {
                    return
                }
            }
        }
    }

    async fn handle_mouse_move(&self, mut mouse_pos: Point) {
        self.local_scale(&mut mouse_pos);
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if EMULATE_TOUCH {
                if obj.handle_touch(TouchPhase::Moved, 0, mouse_pos).await {
                    return
                }
            } else {
                if obj.handle_mouse_move(mouse_pos).await {
                    return
                }
            }
        }
    }

    async fn handle_mouse_wheel(&self, mut wheel_pos: Point) {
        self.local_scale(&mut wheel_pos);
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_mouse_wheel(wheel_pos).await {
                return
            }
        }
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, mut touch_pos: Point) {
        self.local_scale(&mut touch_pos);
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            if obj.handle_touch(phase, id, touch_pos).await {
                return
            }
        }
    }

    #[cfg(target_os = "android")]
    async fn handle_autosuggest(&self, ev: crate::android::AndroidSuggestEvent) {
        use crate::android::AndroidSuggestEvent::*;
        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            let is_handled = match &ev {
                Compose { text, cursor_pos, is_commit } => {
                    obj.handle_compose_text(&text, *is_commit).await
                }
                ComposeRegion { start, end } => obj.handle_set_compose_region(*start, *end).await,
            };
            if is_handled {
                return
            }
        }
    }

    pub async fn draw(&self) {
        let atom = &mut PropertyAtomicGuard::new();
        let trace_id = rand::random();
        let timest = unixtime();

        let local = self.screen_size.get() / self.scale.get();
        let rect = Rectangle::from([0., 0., local.w, local.h]);
        t!("Window::draw({rect:?}) [timest={timest}, trace_id={trace_id}]");

        let mut draw_calls = vec![];
        let mut child_calls = vec![];

        for child in self.get_children() {
            let obj = get_ui_object3(&child);
            let Some(mut draw_update) = obj.draw(rect, trace_id, atom).await else {
                t!("{child:?} draw returned none [trace_id={trace_id}]");
                continue
            };

            draw_calls.append(&mut draw_update.draw_calls);
            child_calls.push(draw_update.key);
        }

        let dc = GfxDrawCall {
            instrs: vec![GfxDrawInstruction::SetScale(self.scale.get())],
            dcs: child_calls,
            z_index: 0,
        };
        draw_calls.push((0, dc));
        //t!("  => {:?}", draw_calls);

        self.render_api.replace_draw_calls(timest, draw_calls);

        t!("Window::draw() - replaced draw call [timest={timest}, trace_id={trace_id}]");
    }
}
