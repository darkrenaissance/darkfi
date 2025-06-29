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
use image::ImageReader;
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::{
    io::Cursor,
    sync::Arc,
};

use crate::{
    gfx::{
        gfxtag, GfxDrawCall, GfxDrawInstruction, GfxDrawMesh, ManagedTexturePtr,
        Rectangle, RenderApi,
    },
    mesh::{MeshBuilder, MeshInfo, COLOR_WHITE},
    prop::{PropertyAtomicGuard, PropertyRect, PropertyStr, PropertyUint32, Role},
    scene::{Pimpl, SceneNodeWeak},
    util::unixtime,
    ExecutorPtr,
};

use super::{DrawTrace, DrawUpdate, OnModify, UIObject};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::image", $($arg)*); } }

pub type ImagePtr = Arc<Image>;

pub struct Image {
    node: SceneNodeWeak,
    render_api: RenderApi,
    tasks: SyncMutex<Vec<smol::Task<()>>>,

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
    pub async fn new(node: SceneNodeWeak, render_api: RenderApi) -> Pimpl {
        t!("Image::new()");

        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let uv = PropertyRect::wrap(node_ref, Role::Internal, "uv").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let path = PropertyStr::wrap(node_ref, Role::Internal, "path", 0).unwrap();

        let self_ = Arc::new(Self {
            node,
            render_api,
            tasks: SyncMutex::new(vec![]),

            texture: SyncMutex::new(None),
            dc_key: OsRng.gen(),

            rect,
            uv,
            z_index,
            priority,
            path,

            parent_rect: SyncMutex::new(None),
        });

        Pimpl::Image(self_)
    }

    async fn reload(self: Arc<Self>) {
        let texture = self.load_texture();
        *self.texture.lock() = Some(texture);

        self.clone().redraw().await;
    }

    fn load_texture(&self) -> ManagedTexturePtr {
        let path = self.path.get();

        // TODO we should NOT use panic here
        let data = Arc::new(SyncMutex::new(vec![]));
        let data2 = data.clone();
        miniquad::fs::load_file(&path.clone(), move |res| match res {
            Ok(res) => *data2.lock() = res,
            Err(e) => {
                error!(target: "ui::image", "Unable to open image: {path}: {e}");
                panic!("Resource not found! {e}");
            }
        });
        let data = std::mem::take(&mut *data.lock());
        let img =
            ImageReader::new(Cursor::new(data)).with_guessed_format().unwrap().decode().unwrap();
        let img = img.to_rgba8();

        //let img = image::ImageReader::open(path).unwrap().decode().unwrap().to_rgba8();

        let width = img.width() as u16;
        let height = img.height() as u16;
        let bmp = img.into_raw();

        self.render_api.new_texture(width, height, bmp, gfxtag!("img"))
    }

    async fn redraw(self: Arc<Self>) {
        let trace: DrawTrace = rand::random();
        let timest = unixtime();
        t!("redraw({:?}) [trace={trace}]", self.node.upgrade().unwrap());
        let Some(parent_rect) = self.parent_rect.lock().clone() else { return };

        let Some(draw_update) = self.get_draw_calls(parent_rect).await else {
            error!(target: "ui::image", "Image failed to draw");
            return
        };
        self.render_api.replace_draw_calls(timest, draw_update.draw_calls);
        t!("redraw() DONE [trace={trace}]");
    }

    /// Called whenever any property changes.
    fn regen_mesh(&self) -> MeshInfo {
        let rect = self.rect.get();
        let uv = self.uv.get();
        let mesh_rect = Rectangle::from([0., 0., rect.w, rect.h]);
        let mut mesh = MeshBuilder::new(gfxtag!("img"));
        mesh.draw_box(&mesh_rect, COLOR_WHITE, &uv);
        mesh.alloc(&self.render_api)
    }

    async fn get_draw_calls(&self, parent_rect: Rectangle) -> Option<DrawUpdate> {
        self.rect.eval(&parent_rect).ok()?;
        let rect = self.rect.get();
        self.uv.eval(&rect).ok()?;

        let mesh = self.regen_mesh();
        let texture = self.texture.lock().clone().expect("Node missing texture_id!");

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
                GfxDrawCall::new(
                    vec![GfxDrawInstruction::Move(rect.pos()), GfxDrawInstruction::Draw(mesh)],
                    vec![],
                    self.z_index.get(),
                    "img",
                ),
            )],
        })
    }
}

#[async_trait]
impl UIObject for Image {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    fn init(&self) {
        *self.texture.lock() = Some(self.load_texture());
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);

        let mut on_modify = OnModify::new(ex, self.node.clone(), me.clone());
        on_modify.when_change(self.rect.prop(), Self::redraw);
        on_modify.when_change(self.uv.prop(), Self::redraw);
        on_modify.when_change(self.z_index.prop(), Self::redraw);
        on_modify.when_change(self.path.prop(), Self::reload);

        *self.tasks.lock() = on_modify.tasks;
    }

    fn stop(&self) {
        self.tasks.lock().clear();
        *self.parent_rect.lock() = None;
        *self.texture.lock() = None;
    }

    async fn draw(
        &self,
        parent_rect: Rectangle,
        trace: DrawTrace,
        _atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        t!("Image::draw() [trace={trace}]");
        *self.parent_rect.lock() = Some(parent_rect);
        self.get_draw_calls(parent_rect).await
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        self.render_api.replace_draw_calls(unixtime(), vec![(self.dc_key, Default::default())]);
    }
}
