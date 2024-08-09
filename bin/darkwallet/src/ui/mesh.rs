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
use std::sync::{Arc, Weak};

use crate::{
    gfx::{DrawCall, DrawInstruction, DrawMesh, Rectangle, RenderApiPtr, Vertex},
    prop::{PropertyPtr, PropertyUint32, Role},
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
    ExecutorPtr,
};

use super::{eval_rect, get_parent_rect, read_rect, DrawUpdate, OnModify, Stoppable};

pub type MeshPtr = Arc<Mesh>;

pub struct Mesh {
    sg: SceneGraphPtr2,
    render_api: RenderApiPtr,
    tasks: Vec<smol::Task<()>>,

    vertex_buffer: miniquad::BufferId,
    index_buffer: miniquad::BufferId,
    // Texture
    num_elements: i32,

    dc_key: u64,

    node_id: SceneNodeId,
    rect: PropertyPtr,
    z_index: PropertyUint32,
}

impl Mesh {
    pub async fn new(
        ex: ExecutorPtr,
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        render_api: RenderApiPtr,
        verts: Vec<Vertex>,
        indices: Vec<u16>,
    ) -> Pimpl {
        let num_elements = indices.len() as i32;
        let vertex_buffer = render_api.new_vertex_buffer(verts).await.unwrap();
        let index_buffer = render_api.new_index_buffer(indices).await.unwrap();

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
                tasks: on_modify.tasks,
                vertex_buffer,
                index_buffer,
                num_elements,
                dc_key: OsRng.gen(),
                node_id,
                rect,
                z_index,
            }
        });

        Pimpl::Mesh(self_)
    }

    async fn redraw(self: Arc<Self>) {
        let sg = self.sg.lock().await;
        let node = sg.get_node(self.node_id).unwrap();

        let Some(parent_rect) = get_parent_rect(&sg, node) else {
            return;
        };

        let Some(draw_update) = self.draw(&sg, &parent_rect) else {
            error!(target: "ui::mesh", "Mesh {:?} failed to draw", node);
            return;
        };
        self.render_api.replace_draw_calls(draw_update.draw_calls).await;
        debug!(target: "ui::mesh", "replace draw calls done");
    }

    pub fn draw(&self, sg: &SceneGraph, parent_rect: &Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::mesh", "Mesh::draw()");
        // Only used for debug messages
        let node = sg.get_node(self.node_id).unwrap();

        let mesh = DrawMesh {
            vertex_buffer: self.vertex_buffer,
            index_buffer: self.index_buffer,
            texture: None,
            num_elements: self.num_elements,
        };

        if let Err(err) = eval_rect(self.rect.clone(), parent_rect) {
            panic!("Node {:?} bad rect property: {}", node, err);
        }

        let Ok(mut rect) = read_rect(self.rect.clone()) else {
            panic!("Node {:?} bad rect property", node);
        };

        rect.x += parent_rect.x;
        rect.y += parent_rect.x;

        let off_x = rect.x / parent_rect.w;
        let off_y = rect.y / parent_rect.h;
        let scale_x = rect.w / parent_rect.w;
        let scale_y = rect.h / parent_rect.h;
        let model = glam::Mat4::from_translation(glam::Vec3::new(off_x, off_y, 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(scale_x, scale_y, 1.));

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                DrawCall {
                    instrs: vec![DrawInstruction::ApplyMatrix(model), DrawInstruction::Draw(mesh)],
                    dcs: vec![],
                    z_index: self.z_index.get(),
                },
            )],
            freed_textures: vec![],
            freed_buffers: vec![],
        })
    }
}

impl Stoppable for Mesh {
    async fn stop(&self) {
        // TODO: Delete own draw call

        // Free buffers
        // Should this be in drop?
        self.render_api.delete_buffer(self.vertex_buffer);
        self.render_api.delete_buffer(self.index_buffer);
    }
}
