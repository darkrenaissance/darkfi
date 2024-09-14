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

use rand::{rngs::OsRng, Rng};
use std::sync::{Arc, Mutex as SyncMutex, Weak};

use crate::{
    error::{Error, Result},
    expr::{Op, SExprCode, SExprMachine, SExprVal},
    gfx::{
        GfxBufferId, GfxDrawCall, GfxDrawInstruction, GfxDrawMesh, Rectangle, RenderApiPtr, Vertex,
    },
    mesh::Color,
    prop::{PropertyPtr, PropertyUint32, Role},
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
    util::enumerate,
    ExecutorPtr,
};

use super::{eval_rect, get_parent_rect, read_rect, DrawUpdate, OnModify, Stoppable};

pub mod shape;
use shape::VectorShape;

pub type VectorArtPtr = Arc<VectorArt>;

pub struct VectorArt {
    sg: SceneGraphPtr2,
    render_api: RenderApiPtr,
    _tasks: Vec<smol::Task<()>>,

    shape: VectorShape,
    buffers: SyncMutex<Option<GfxDrawMesh>>,

    dc_key: u64,

    node_id: SceneNodeId,
    rect: PropertyPtr,
    z_index: PropertyUint32,
}

impl VectorArt {
    pub async fn new(
        ex: ExecutorPtr,
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        render_api: RenderApiPtr,
        shape: VectorShape,
    ) -> Pimpl {
        let scene_graph = sg.lock().await;
        let node = scene_graph.get_node(node_id).unwrap();
        let node_name = node.name.clone();
        let rect = node.get_property("rect").expect("Mesh::rect");
        let z_index = PropertyUint32::wrap(node, Role::Internal, "z_index", 0).unwrap();
        drop(scene_graph);

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            let mut on_modify = OnModify::new(ex, node_name, node_id, me.clone());
            on_modify.when_change(rect.clone(), Self::redraw);
            on_modify.when_change(z_index.prop(), Self::redraw);

            Self {
                sg,
                render_api,
                _tasks: on_modify.tasks,
                shape,
                buffers: SyncMutex::new(None),
                dc_key: OsRng.gen(),
                node_id,
                rect,
                z_index,
            }
        });

        Pimpl::VectorArt(self_)
    }

    async fn redraw(self: Arc<Self>) {
        let sg = self.sg.lock().await;
        let node = sg.get_node(self.node_id).unwrap();

        let Some(parent_rect) = get_parent_rect(&sg, node) else {
            return;
        };

        let Some(draw_update) = self.draw(&sg, &parent_rect) else {
            error!(target: "ui::vector_art", "Mesh {:?} failed to draw", node);
            return;
        };
        self.render_api.replace_draw_calls(draw_update.draw_calls);
        debug!(target: "ui::vector_art", "replace draw calls done");
    }

    pub fn draw(&self, sg: &SceneGraph, parent_rect: &Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::vector_art", "VectorArt::draw()");
        // Only used for debug messages
        let node = sg.get_node(self.node_id).unwrap();

        if let Err(err) = eval_rect(self.rect.clone(), parent_rect) {
            panic!("Node {:?} bad rect property: {}", node, err);
        }

        let Ok(mut rect) = read_rect(self.rect.clone()) else {
            panic!("Node {:?} bad rect property", node);
        };

        rect.x += parent_rect.x;
        rect.y += parent_rect.x;

        let verts = self.shape.eval(rect.w, rect.h).expect("bad shape");

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

        let off_x = rect.x / parent_rect.w;
        let off_y = rect.y / parent_rect.h;
        let scale_x = 1. / parent_rect.w;
        let scale_y = 1. / parent_rect.h;
        let model = glam::Mat4::from_translation(glam::Vec3::new(off_x, off_y, 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(scale_x, scale_y, 1.));

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                GfxDrawCall {
                    instrs: vec![
                        GfxDrawInstruction::ApplyMatrix(model),
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

impl Stoppable for VectorArt {
    async fn stop(&self) {
        // TODO: Delete own draw call

        // Free buffers
        // Should this be in drop?
        if let Some(mesh) = &*self.buffers.lock().unwrap() {
            self.render_api.delete_buffer(mesh.vertex_buffer);
            self.render_api.delete_buffer(mesh.index_buffer);
        }
    }
}
