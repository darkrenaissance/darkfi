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

//use async_lock::Mutex;
use miniquad::{BufferId, TextureId};
use rand::{rngs::OsRng, Rng};
use std::sync::{Arc, Mutex as SyncMutex, Weak};

use crate::{
    gfx2::{DrawCall, DrawInstruction, DrawMesh, Rectangle, RenderApi, RenderApiPtr, Vertex},
    mesh::{Color, MeshBuilder, MeshInfo, COLOR_BLUE, COLOR_WHITE},
    prop::{
        PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyStr, PropertyUint32,
    },
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
    text2::{self, Glyph, GlyphPositionIter, SpritePtr, TextShaper, TextShaperPtr},
    util::zip3,
};

use super::{eval_rect, get_parent_rect, read_rect, DrawUpdate, OnModify, Stoppable};

pub type ImagePtr = Arc<Image>;

pub struct Image {
    sg: SceneGraphPtr2,
    render_api: RenderApiPtr,
    tasks: Vec<smol::Task<()>>,

    mesh: SyncMutex<Option<MeshInfo>>,
    texture: SyncMutex<Option<TextureId>>,
    dc_key: u64,

    node_id: SceneNodeId,
    rect: PropertyPtr,
    z_index: PropertyUint32,
    path: PropertyStr,
}

impl Image {
    pub async fn new(
        ex: Arc<smol::Executor<'static>>,
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        render_api: RenderApiPtr,
    ) -> Pimpl {
        let scene_graph = sg.lock().await;
        let node = scene_graph.get_node(node_id).unwrap();
        let node_name = node.name.clone();
        let rect = node.get_property("rect").expect("Text::rect");
        let z_index = PropertyUint32::wrap(node, "z_index", 0).unwrap();
        let path = PropertyStr::wrap(node, "path", 0).unwrap();
        drop(scene_graph);

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            let mut on_modify = OnModify::new(ex, node_name, node_id, me.clone());
            on_modify.when_change(rect.clone(), Self::redraw);
            on_modify.when_change(z_index.prop(), Self::redraw);
            on_modify.when_change(path.prop(), Self::reload);

            Self {
                sg,
                render_api,
                tasks: on_modify.tasks,
                mesh: SyncMutex::new(None),
                texture: SyncMutex::new(None),
                dc_key: OsRng.gen(),
                node_id,
                rect,
                z_index,
                path,
            }
        });

        *self_.texture.lock().unwrap() = Some(self_.load_texture().await);

        Pimpl::Image(self_)
    }

    async fn reload(self: Arc<Self>) {
        let texture = self.load_texture().await;
        let old_texture = std::mem::replace(&mut *self.texture.lock().unwrap(), Some(texture));

        self.clone().redraw().await;

        if let Some(old_texture) = old_texture {
            self.render_api.delete_texture(old_texture);
        }
    }

    async fn load_texture(&self) -> TextureId {
        let path = self.path.get();
        // TODO we should NOT use unwrap here
        let img = image::ImageReader::open(path).unwrap().decode().unwrap().to_rgba8();
        let width = img.width() as u16;
        let height = img.height() as u16;
        let bmp = img.into_raw();

        let texture_id = self.render_api.new_texture(width, height, bmp).await.unwrap();
        texture_id
    }

    async fn redraw(self: Arc<Self>) {
        let sg = self.sg.lock().await;
        let node = sg.get_node(self.node_id).unwrap();

        let Some(parent_rect) = get_parent_rect(&sg, node) else {
            return;
        };

        let Some(draw_update) = self.draw(&sg, &parent_rect).await else {
            error!(target: "ui::text", "Text {:?} failed to draw", node);
            return;
        };
        self.render_api.replace_draw_calls(draw_update.draw_calls).await;
        debug!(target: "ui::text", "replace draw calls done");
    }

    /// Called whenever any property changes.
    async fn regen_mesh(&self, clip: Rectangle) -> MeshInfo {
        let basic = Rectangle { x: 0., y: 0., w: 1., h: 1. };

        let mut mesh = MeshBuilder::new();
        mesh.draw_box(&basic, COLOR_WHITE, &basic);
        mesh.alloc(&self.render_api).await.unwrap()
    }

    pub async fn draw(&self, sg: &SceneGraph, parent_rect: &Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::text", "Text::draw()");
        // Only used for debug messages
        let node = sg.get_node(self.node_id).unwrap();

        if let Err(err) = eval_rect(self.rect.clone(), parent_rect) {
            panic!("Node {:?} bad rect property: {}", node, err);
        }

        let Ok(mut rect) = read_rect(self.rect.clone()) else {
            panic!("Node {:?} bad rect property", node);
        };

        // draw will recalc this when it's None
        let mesh = self.regen_mesh(rect.clone()).await;
        let old_mesh = std::mem::replace(&mut *self.mesh.lock().unwrap(), Some(mesh.clone()));

        let Some(texture_id) = *self.texture.lock().unwrap() else {
            panic!("Node {:?} missing texture_id!", node);
        };

        // We're finished with these so clean up.
        let mut freed_buffers = vec![];
        if let Some(old) = old_mesh {
            freed_buffers.push(old.vertex_buffer);
            freed_buffers.push(old.index_buffer);
        }

        let mesh = DrawMesh {
            vertex_buffer: mesh.vertex_buffer,
            index_buffer: mesh.index_buffer,
            texture: Some(texture_id),
            num_elements: mesh.num_elements,
        };

        let off_x = rect.x / parent_rect.w;
        let off_y = rect.y / parent_rect.h;
        // We could use pixels here if we want to. No difference really.
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
            freed_buffers,
        })
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        // TODO: Delete own draw call

        // Free buffers
        // Should this be in drop?
        if let Some(mesh) = &*self.mesh.lock().unwrap() {
            let vertex_buffer = mesh.vertex_buffer;
            let index_buffer = mesh.index_buffer;
            self.render_api.delete_buffer(vertex_buffer);
            self.render_api.delete_buffer(index_buffer);
        }
        let texture_id = self.texture.lock().unwrap().unwrap();
        self.render_api.delete_texture(texture_id);
    }
}
