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

use std::sync::{Arc, Weak};

use crate::{
    error::{Error, Result},
    expr::{SExprMachine, SExprVal},
    gfx2::{DrawCall, Rectangle},
    prop::PropertyPtr,
    scene::{SceneGraph, SceneNode, SceneNodeId, SceneNodeType},
};

mod chatview;
pub use chatview::{ChatView, ChatViewPtr};
mod editbox;
pub use editbox::{EditBox, EditBoxPtr};
mod mesh;
pub use mesh::{Mesh, MeshPtr};
mod layer;
pub use layer::{RenderLayer, RenderLayerPtr};
mod text;
pub use text::{Text, TextPtr};
mod win;
pub use win::{Window, WindowPtr};

pub trait Stoppable {
    async fn stop(&self);
}

pub struct DrawUpdate {
    pub key: u64,
    pub draw_calls: Vec<(u64, DrawCall)>,
}

pub struct OnModify<T> {
    ex: Arc<smol::Executor<'static>>,
    node_name: String,
    node_id: SceneNodeId,
    me: Weak<T>,
    pub tasks: Vec<smol::Task<()>>,
}

impl<T: Send + Sync + 'static> OnModify<T> {
    pub fn new(
        ex: Arc<smol::Executor<'static>>,
        node_name: String,
        node_id: SceneNodeId,
        me: Weak<T>,
    ) -> Self {
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
                let _ = on_modify_sub.receive().await;
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

pub fn eval_rect(rect: PropertyPtr, parent_rect: &Rectangle) -> Result<()> {
    if rect.array_len != 4 {
        return Err(Error::PropertyWrongLen)
    }

    for i in 0..4 {
        if !rect.is_expr(i)? {
            continue
        }

        let expr = rect.get_expr(i).unwrap();

        let machine = SExprMachine {
            globals: vec![
                ("w".to_string(), SExprVal::Float32(parent_rect.w)),
                ("h".to_string(), SExprVal::Float32(parent_rect.h)),
            ],
            stmts: &expr,
        };

        let v = machine.call()?.as_f32()?;
        rect.set_cache_f32(i, v).unwrap();
    }
    Ok(())
}

pub fn read_rect(rect_prop: PropertyPtr) -> Result<Rectangle> {
    if rect_prop.array_len != 4 {
        return Err(Error::PropertyWrongLen)
    }

    let mut rect = [0.; 4];
    for i in 0..4 {
        if rect_prop.is_expr(i)? {
            rect[i] = rect_prop.get_cached(i)?.as_f32()?;
        } else {
            rect[i] = rect_prop.get_f32(i)?;
        }
    }
    Ok(Rectangle::from_array(rect))
}

pub fn get_parent_rect(sg: &SceneGraph, node: &SceneNode) -> Option<Rectangle> {
    // read our parent
    if node.parents.is_empty() {
        info!("RenderLayer {:?} has no parents so skipping", node);
        return None
    }
    if node.parents.len() != 1 {
        error!("RenderLayer {:?} has too many parents so skipping", node);
        return None
    }
    let parent_id = node.parents[0].id;
    let parent_node = sg.get_node(parent_id).unwrap();
    let parent_rect = match parent_node.typ {
        SceneNodeType::Window => {
            let Some(screen_size_prop) = parent_node.get_property("screen_size") else {
                error!(
                    "RenderLayer {:?} parent node {:?} missing screen_size property",
                    node, parent_node
                );
                return None
            };
            let screen_width = screen_size_prop.get_f32(0).unwrap();
            let screen_height = screen_size_prop.get_f32(1).unwrap();

            let parent_rect = Rectangle::from_array([0., 0., screen_width, screen_height]);
            parent_rect
        }
        SceneNodeType::RenderLayer => {
            // get their rect property
            let Some(parent_rect) = parent_node.get_property("rect") else {
                error!(
                    "RenderLayer {:?} parent node {:?} missing rect property",
                    node, parent_node
                );
                return None
            };
            // read parent's rect
            let Ok(parent_rect) = read_rect(parent_rect) else {
                error!(
                    "RenderLayer {:?} parent node {:?} malformed rect property",
                    node, parent_node
                );
                return None
            };
            parent_rect
        }
        _ => {
            error!(
                "RenderLayer {:?} parent node {:?} wrong type {:?}",
                node, parent_node, parent_node.typ
            );
            return None
        }
    };
    Some(parent_rect)
}
