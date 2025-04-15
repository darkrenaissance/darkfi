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
use futures::stream::{FuturesUnordered, StreamExt};
use log::{log_enabled, Level::Trace};
use miniquad::{KeyCode, KeyMods, MouseButton, TouchPhase};
use std::{
    fmt::Debug,
    sync::{Arc, Weak},
};

use crate::{
    error::{Error, Result},
    expr::{SExprMachine, SExprVal},
    gfx::{GfxBufferId, GfxDrawCall, GfxDrawMesh, GfxTextureId, Point, Rectangle},
    prop::{ModifyAction, PropertyAtomicGuard, PropertyPtr, Role},
    scene::{Pimpl, SceneNode as SceneNode3, SceneNodeId, SceneNodePtr, SceneNodeWeak},
    AndroidSuggestEvent, ExecutorPtr,
};

mod button;
pub use button::{Button, ButtonPtr};
pub mod chatview;
pub use chatview::{ChatView, ChatViewPtr};
mod chatedit;
pub use chatedit::{ChatEdit, ChatEditPtr};
mod editbox;
pub use editbox::{EditBox, EditBoxPtr};
pub mod emoji_picker;
pub use emoji_picker::{EmojiPicker, EmojiPickerPtr};
mod gesture;
pub use gesture::{Gesture, GesturePtr};
mod image;
pub use image::{Image, ImagePtr};
mod vector_art;
pub use vector_art::{
    shape::{ShapeVertex, VectorShape},
    VectorArt, VectorArtPtr,
};
mod layer;
pub use layer::{Layer, LayerPtr};
mod shortcut;
pub use shortcut::{Shortcut, ShortcutPtr};
mod text;
pub use text::{Text, TextPtr};
mod win;
pub use win::{Window, WindowPtr};

macro_rules! e { ($($arg:tt)*) => { error!(target: "scene::on_modify", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "scene::on_modify", $($arg)*); } }

#[async_trait]
pub trait UIObject: Sync {
    fn priority(&self) -> u32;

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {}

    async fn draw(
        &self,
        parent_rect: Rectangle,
        trace_id: u32,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        None
    }

    async fn handle_char(&self, key: char, mods: KeyMods, repeat: bool) -> bool {
        false
    }
    async fn handle_key_down(&self, key: KeyCode, mods: KeyMods, repeat: bool) -> bool {
        false
    }
    async fn handle_key_up(&self, key: KeyCode, mods: KeyMods) -> bool {
        false
    }
    async fn handle_mouse_btn_down(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        false
    }
    async fn handle_mouse_btn_up(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        false
    }
    async fn handle_mouse_move(&self, mouse_pos: Point) -> bool {
        false
    }
    async fn handle_mouse_wheel(&self, wheel_pos: Point) -> bool {
        false
    }
    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_pos: Point) -> bool {
        false
    }
}

pub struct DrawUpdate {
    pub key: u64,
    pub draw_calls: Vec<(u64, GfxDrawCall)>,
}

pub struct OnModify<T> {
    ex: ExecutorPtr,
    node: SceneNodeWeak,
    me: Weak<T>,
    pub tasks: Vec<smol::Task<()>>,
}

impl<T: Send + Sync + 'static> OnModify<T> {
    pub fn new(ex: ExecutorPtr, node: SceneNodeWeak, me: Weak<T>) -> Self {
        Self { ex, node, me, tasks: vec![] }
    }

    pub fn when_change<F>(&mut self, prop: PropertyPtr, f: impl Fn(Arc<T>) -> F + Send + 'static)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let mut on_modify_subs = vec![(Arc::downgrade(&prop), None, prop.subscribe_modify())];
        for dep in prop.get_depends() {
            let Some(dep_prop) = dep.prop.upgrade() else { continue };
            on_modify_subs.push((dep.prop, Some(dep.i), dep_prop.subscribe_modify()));
        }

        let me = self.me.clone();
        let node = self.node.clone();
        let task = self.ex.spawn(async move {
            loop {
                let mut poll_queues = FuturesUnordered::new();
                for (i, (prop_weak, prop_i, on_modify_sub)) in on_modify_subs.iter().enumerate() {
                    let recv = on_modify_sub.receive();
                    poll_queues.push(async move {
                        let (role, action) = recv.await.ok()?;
                        Some((i, prop_weak, prop_i, role, action))
                    });
                }

                let Some(Some((idx, prop_weak, prop_i, role, action))) = poll_queues.next().await else {
                    e!("Property {:?} on_modify pipe is broken", prop);
                    return
                };

                // Skip internal messages from ourselves or explicitly marked ignored
                if (idx == 0 && role == Role::Internal) || role == Role::Ignored{
                    continue
                }
                if let Some(prop_i) = prop_i {
                    match action {
                        ModifyAction::Set(i) => if *prop_i != i { continue },
                        ModifyAction::SetCache(idxs) => if !idxs.contains(prop_i) { continue },
                        _ => continue
                    }
                }

                if (idx == 0) {
                    t!("Property {:?} modified [depend_idx={idx}, role={role:?}]", prop);
                } else {
                    t!(
                        "Property {:?} modified -> triggering {:?} [depend_idx={idx}, role={role:?}]",
                        prop_weak.upgrade().unwrap(),
                        prop
                    );
                }

                let Some(self_) = me.upgrade() else {
                    // Should not happen
                    panic!("{:?} self destroyed before modify_task was stopped!", prop);
                };

                //debug!(target: "app", "property modified");
                f(self_).await;
            }
        });
        self.tasks.push(task);
    }
}

pub fn get_ui_object_ptr(node: &SceneNode3) -> Arc<dyn UIObject + Send> {
    match &node.pimpl {
        Pimpl::Layer(obj) => obj.clone(),
        Pimpl::VectorArt(obj) => obj.clone(),
        Pimpl::Text(obj) => obj.clone(),
        Pimpl::EditBox(obj) => obj.clone(),
        Pimpl::ChatEdit(obj) => obj.clone(),
        Pimpl::ChatView(obj) => obj.clone(),
        Pimpl::Image(obj) => obj.clone(),
        Pimpl::Button(obj) => obj.clone(),
        Pimpl::EmojiPicker(obj) => obj.clone(),
        Pimpl::Shortcut(obj) => obj.clone(),
        Pimpl::Gesture(obj) => obj.clone(),
        _ => panic!("unhandled type for get_ui_object: {node:?}"),
    }
}
pub fn get_ui_object3<'a>(node: &'a SceneNode3) -> &'a dyn UIObject {
    match &node.pimpl {
        Pimpl::Layer(obj) => obj.as_ref(),
        Pimpl::VectorArt(obj) => obj.as_ref(),
        Pimpl::Text(obj) => obj.as_ref(),
        Pimpl::EditBox(obj) => obj.as_ref(),
        Pimpl::ChatEdit(obj) => obj.as_ref(),
        Pimpl::ChatView(obj) => obj.as_ref(),
        Pimpl::Image(obj) => obj.as_ref(),
        Pimpl::Button(obj) => obj.as_ref(),
        Pimpl::EmojiPicker(obj) => obj.as_ref(),
        Pimpl::Shortcut(obj) => obj.as_ref(),
        Pimpl::Gesture(obj) => obj.as_ref(),
        _ => panic!("unhandled type for get_ui_object: {node:?}"),
    }
}

pub fn get_children_ordered(node: &SceneNode3) -> Vec<SceneNodePtr> {
    let mut child_infs = vec![];
    for child in node.get_children() {
        let obj = get_ui_object3(&child);
        let priority = obj.priority();
        child_infs.push((child, priority));
    }
    child_infs.sort_unstable_by_key(|(_, priority)| *priority);

    let nodes = child_infs.into_iter().rev().map(|(node, _)| node).collect();
    nodes
}
