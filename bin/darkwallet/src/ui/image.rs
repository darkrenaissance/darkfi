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
use image::ImageReader;
use rand::{rngs::OsRng, Rng};
use std::{
    io::Cursor,
    sync::{Arc, Mutex as SyncMutex, OnceLock, Weak},
};

use crate::{
    gfx::{
        GfxDrawCall, GfxDrawInstruction, GfxDrawMesh, GfxTextureId, ManagedTexturePtr, Rectangle,
        RenderApi,
    },
    mesh::{MeshBuilder, MeshInfo, COLOR_WHITE},
    prop::{PropertyPtr, PropertyRect, PropertyStr, PropertyUint32, Role},
    scene::{Pimpl, SceneNodePtr, SceneNodeWeak},
    util::unixtime,
    ExecutorPtr,
};

use super::{DrawUpdate, OnModify, UIObject};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::image", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::image", $($arg)*); } }

pub type ImagePtr = Arc<Image>;

pub struct Image {
    node: SceneNodeWeak,
    render_api: RenderApi,
    tasks: OnceLock<Vec<smol::Task<()>>>,

    texture: SyncMutex<Option<ManagedTexturePtr>>,
    dc_key: u64,

    rect: PropertyRect,
    uv: PropertyRect,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    path: PropertyStr,

    parent_rect: SyncMutex<Option<Rectangle>>,
}

impl Image {
    pub async fn new(node: SceneNodeWeak, render_api: RenderApi, ex: ExecutorPtr) -> Pimpl {
        t!("Image::new()");

        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let uv = PropertyRect::wrap(node_ref, Role::Internal, "uv").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let path = PropertyStr::wrap(node_ref, Role::Internal, "path", 0).unwrap();

        let node_name = node_ref.name.clone();
        let node_id = node_ref.id;

        let self_ = Arc::new(Self {
            node,
            render_api,
            tasks: OnceLock::new(),

            texture: SyncMutex::new(None),
            dc_key: OsRng.gen(),

            rect,
            uv,
            z_index,
            priority,
            path,

            parent_rect: SyncMutex::new(None),
        });

        *self_.texture.lock().unwrap() = Some(self_.load_texture());

        Pimpl::Image(self_)
    }

    async fn reload(self: Arc<Self>) {
        let texture = self.load_texture();
        let old_texture = std::mem::replace(&mut *self.texture.lock().unwrap(), Some(texture));

        self.clone().redraw().await;
    }

    fn load_texture(&self) -> ManagedTexturePtr {
        let path = self.path.get();

        // TODO we should NOT use panic here
        let data = Arc::new(SyncMutex::new(vec![]));
        let data2 = data.clone();
        miniquad::fs::load_file(&path.clone(), move |res| match res {
            Ok(res) => *data2.lock().unwrap() = res,
            Err(e) => {
                error!(target: "ui::image", "Unable to open image: {path}");
                panic!("Resource not found!");
            }
        });
        let data = std::mem::take(&mut *data.lock().unwrap());
        let img =
            ImageReader::new(Cursor::new(data)).with_guessed_format().unwrap().decode().unwrap();
        let img = img.to_rgba8();

        //let img = image::ImageReader::open(path).unwrap().decode().unwrap().to_rgba8();

        let width = img.width() as u16;
        let height = img.height() as u16;
        let bmp = img.into_raw();

        self.render_api.new_texture(width, height, bmp)
    }

    async fn redraw(self: Arc<Self>) {
        let trace_id = rand::random();
        let timest = unixtime();
        t!("redraw({:?}) [trace_id={trace_id}]", self.node.upgrade().unwrap());
        let Some(parent_rect) = self.parent_rect.lock().unwrap().clone() else { return };

        let Some(draw_update) = self.get_draw_calls(parent_rect, trace_id).await else {
            error!(target: "ui::image", "Image failed to draw");
            return;
        };
        self.render_api.replace_draw_calls(timest, draw_update.draw_calls);
        t!("redraw() DONE [trace_id={trace_id}]");
    }

    /// Called whenever any property changes.
    fn regen_mesh(&self) -> MeshInfo {
        let rect = self.rect.get();
        let uv = self.uv.get();
        let mesh_rect = Rectangle::from([0., 0., rect.w, rect.h]);
        let mut mesh = MeshBuilder::new();
        mesh.draw_box(&mesh_rect, COLOR_WHITE, &uv);
        mesh.alloc(&self.render_api)
    }

    async fn get_draw_calls(&self, parent_rect: Rectangle, _: u32) -> Option<DrawUpdate> {
        self.rect.eval(&parent_rect).ok()?;
        let rect = self.rect.get();
        self.uv.eval(&rect).ok()?;

        let mesh = self.regen_mesh();
        let texture = self.texture.lock().unwrap().clone().expect("Node missing texture_id!");

        let mesh = GfxDrawMesh {
            vertex_buffer: mesh.vertex_buffer,
            index_buffer: mesh.index_buffer,
            texture: Some(texture),
            num_elements: mesh.num_elements,
        };

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
        })
    }
}

#[async_trait]
impl UIObject for Image {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);

        let mut on_modify = OnModify::new(ex, self.node.clone(), me.clone());
        on_modify.when_change(self.rect.prop(), Self::redraw);
        on_modify.when_change(self.uv.prop(), Self::redraw);
        on_modify.when_change(self.z_index.prop(), Self::redraw);
        on_modify.when_change(self.path.prop(), Self::reload);

        self.tasks.set(on_modify.tasks);
    }

    async fn draw(&self, parent_rect: Rectangle, trace_id: u32) -> Option<DrawUpdate> {
        t!("Image::draw() [trace_id={trace_id}]");
        *self.parent_rect.lock().unwrap() = Some(parent_rect);
        self.get_draw_calls(parent_rect, trace_id).await
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        self.render_api.replace_draw_calls(unixtime(), vec![(self.dc_key, Default::default())]);
    }
}
