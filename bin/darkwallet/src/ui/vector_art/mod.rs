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
use rand::{rngs::OsRng, Rng};
use std::sync::{Arc, Mutex as SyncMutex, Weak};

use crate::{
    error::{Error, Result},
    expr::{Op, SExprCode, SExprMachine, SExprVal},
    gfx::{
        GfxBufferId, GfxDrawCall, GfxDrawInstruction, GfxDrawMesh, Rectangle, RenderApiPtr, Vertex,
    },
    mesh::Color,
    prop::{PropertyFloat32, PropertyPtr, PropertyRect, PropertyUint32, Role},
    scene::{Pimpl, SceneNodePtr, SceneNodeWeak},
    util::enumerate,
    ExecutorPtr,
};

use super::{DrawUpdate, OnModify, UIObject};

pub mod shape;
use shape::VectorShape;

pub type VectorArtPtr = Arc<VectorArt>;

pub struct VectorArt {
    node: SceneNodeWeak,
    render_api: RenderApiPtr,
    _tasks: Vec<smol::Task<()>>,

    shape: VectorShape,
    buffers: SyncMutex<Option<GfxDrawMesh>>,
    dc_key: u64,

    rect: PropertyRect,
    z_index: PropertyUint32,

    parent_rect: SyncMutex<Option<Rectangle>>,
}

impl VectorArt {
    pub async fn new(
        node: SceneNodeWeak,
        shape: VectorShape,
        render_api: RenderApiPtr,
        ex: ExecutorPtr,
    ) -> Pimpl {
        debug!(target: "ui::vector_art", "VectorArt::new()");

        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();

        let node_name = node_ref.name.clone();
        let node_id = node_ref.id;

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            let mut on_modify = OnModify::new(ex, node_name, node_id, me.clone());
            on_modify.when_change(rect.prop(), Self::redraw);
            on_modify.when_change(z_index.prop(), Self::redraw);

            Self {
                node,
                render_api,
                _tasks: on_modify.tasks,

                shape,
                buffers: SyncMutex::new(None),
                dc_key: OsRng.gen(),

                rect,
                z_index,

                parent_rect: SyncMutex::new(None),
            }
        });

        Pimpl::VectorArt(self_)
    }

    async fn redraw(self: Arc<Self>) {
        let Some(parent_rect) = self.parent_rect.lock().unwrap().clone() else { return };

        let Some(draw_update) = self.get_draw_calls(parent_rect).await else {
            error!(target: "ui::vector_art", "Mesh failed to draw");
            return;
        };
        self.render_api.replace_draw_calls(draw_update.draw_calls);
        assert!(draw_update.freed_textures.is_empty());
        for buff in draw_update.freed_buffers {
            self.render_api.delete_buffer(buff);
        }
        debug!(target: "ui::vector_art", "replace draw calls done");
    }

    async fn get_draw_calls(&self, parent_rect: Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::vector_art", "VectorArt::draw_cached()");
        self.rect.eval(&parent_rect).ok()?;
        let rect = self.rect.get();
        let verts = self.shape.eval(rect.w, rect.h).expect("bad shape");

        //debug!(target: "ui::vector_art", "=> {verts:#?}");
        let vertex_buffer = self.render_api.new_vertex_buffer(verts);
        // You are one lazy motherfucker
        let index_buffer = self.render_api.new_index_buffer(self.shape.indices.clone());
        let mesh = GfxDrawMesh {
            vertex_buffer,
            index_buffer,
            texture: None,
            num_elements: self.shape.indices.len() as i32,
        };

        let old_mesh = std::mem::replace(&mut *self.buffers.lock().unwrap(), Some(mesh.clone()));
        let mut freed_buffers = vec![];
        if let Some(old_mesh) = old_mesh {
            freed_buffers.push(old_mesh.vertex_buffer);
            freed_buffers.push(old_mesh.index_buffer);
        }

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                GfxDrawCall {
                    instrs: vec![
                        GfxDrawInstruction::Move(rect.pos()),
                        GfxDrawInstruction::Draw(mesh),
                    ],
                    dcs: vec![],
                    z_index: self.z_index.get(),
                },
            )],
            freed_textures: vec![],
            freed_buffers,
        })
    }
}

#[async_trait]
impl UIObject for VectorArt {
    fn z_index(&self) -> u32 {
        self.z_index.get()
    }

    async fn draw(&self, parent_rect: Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::vector_art", "VectorArt::draw()");
        *self.parent_rect.lock().unwrap() = Some(parent_rect);
        self.get_draw_calls(parent_rect).await
    }
}

/*
impl Stoppable for VectorArt {
    async fn stop(&self) {
        // TODO: Delete own draw call

        // Free buffers
        // Should this be in drop? ---> yes it should
        if let Some(mesh) = &*self.buffers.lock().unwrap() {
            self.render_api.delete_buffer(mesh.vertex_buffer);
            self.render_api.delete_buffer(mesh.index_buffer);
        }
    }
}
*/
