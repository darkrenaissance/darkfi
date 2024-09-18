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
use miniquad::{KeyCode, KeyMods, MouseButton, TouchPhase};
use std::sync::{Arc, Weak};

use crate::{
    error::{Error, Result},
    expr::{SExprMachine, SExprVal},
    gfx::{GfxBufferId, GfxDrawCall, GfxTextureId, Point, Rectangle},
    prop::{PropertyPtr, Role},
    scene::{Pimpl, SceneNode as SceneNode3, SceneNodeId, SceneNodePtr},
    ExecutorPtr,
};

mod button;
pub use button::{Button, ButtonPtr};
pub mod chatview;
pub use chatview::{ChatView, ChatViewPtr};
mod editbox;
pub use editbox::{EditBox, EditBoxPtr};
mod image;
pub use image::{Image, ImagePtr};
pub mod vector_art;
pub use vector_art::{
    shape::{ShapeVertex, VectorShape},
    VectorArt, VectorArtPtr,
};
mod layer;
pub use layer::{Layer, LayerPtr};
mod text;
pub use text::{Text, TextPtr};
mod win;
pub use win::{Window, WindowPtr};

#[async_trait]
pub trait UIObject: Sync {
    fn z_index(&self) -> u32;

    async fn draw(&self, parent_rect: Rectangle) -> Option<DrawUpdate> {
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
    pub freed_textures: Vec<GfxTextureId>,
    pub freed_buffers: Vec<GfxBufferId>,
}

pub struct OnModify<T> {
    ex: ExecutorPtr,
    node_name: String,
    node_id: SceneNodeId,
    me: Weak<T>,
    pub tasks: Vec<smol::Task<()>>,
}

impl<T: Send + Sync + 'static> OnModify<T> {
    pub fn new(ex: ExecutorPtr, node_name: String, node_id: SceneNodeId, me: Weak<T>) -> Self {
        Self { ex, node_name, node_id, me, tasks: vec![] }
    }

    pub fn when_change<F>(&mut self, prop: PropertyPtr, f: impl Fn(Arc<T>) -> F + Send + 'static)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let node_name = self.node_name.clone();
        let node_id = self.node_id;
        let on_modify_sub = prop.subscribe_modify();
        let prop_name = prop.name.clone();
        let me = self.me.clone();
        let task = self.ex.spawn(async move {
            loop {
                let Ok((role, _)) = on_modify_sub.receive().await else {
                    error!(target: "app", "Property '{}':{}/'{}' on_modify pipe is broken", node_name, node_id, prop_name);
                    return
                };

                if role == Role::Internal {
                    continue
                }

                debug!(target: "app", "Property '{}':{}/'{}' modified", node_name, node_id, prop_name);

                let Some(self_) = me.upgrade() else {
                    // Should not happen
                    panic!(
                        "'{}':{}/'{}' self destroyed before modify_task was stopped!",
                        node_name, node_id, prop_name
                    );
                };

                debug!(target: "app", "property modified");
                f(self_).await;
            }
        });
        self.tasks.push(task);
    }
}

pub fn get_ui_object3<'a>(node: &'a SceneNode3) -> &'a dyn UIObject {
    match &node.pimpl {
        Pimpl::Layer(obj) => obj.as_ref(),
        Pimpl::VectorArt(obj) => obj.as_ref(),
        Pimpl::Text(obj) => obj.as_ref(),
        Pimpl::EditBox(obj) => obj.as_ref(),
        Pimpl::ChatView(obj) => obj.as_ref(),
        Pimpl::Image(obj) => obj.as_ref(),
        Pimpl::Button(obj) => obj.as_ref(),
        _ => panic!("unhandled type for get_ui_object"),
    }
}

pub fn get_children_ordered(node: &SceneNode3) -> Vec<SceneNodePtr> {
    let mut child_infs = vec![];
    for child in node.get_children() {
        let obj = get_ui_object3(&child);
        let z_index = obj.z_index();
        child_infs.push((child, z_index));
    }
    child_infs.sort_unstable_by_key(|(_, z_index)| *z_index);

    let nodes = child_infs.into_iter().rev().map(|(node, _)| node).collect();
    nodes
}
