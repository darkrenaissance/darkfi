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
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use crate::{
    gfx::{
        anim::{SequenceAnimation, SequenceAnimationFrame},
        gfxtag, DrawCall, DrawInstruction, DrawMesh, ManagedTexturePtr, Rectangle, RenderApi,
    },
    mesh::{MeshBuilder, MeshInfo, COLOR_WHITE},
    prop::{BatchGuardPtr, PropertyAtomicGuard, PropertyRect, PropertyStr, PropertyUint32, Role},
    scene::{Pimpl, SceneNodeWeak},
    util::unixtime,
    ExecutorPtr,
};

use super::{DrawTrace, DrawUpdate, OnModify, UIObject};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::video", $($arg)*); } }

pub type VideoPtr = Arc<Video>;

pub struct Video {
    node: SceneNodeWeak,
    render_api: RenderApi,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    stop_load: Arc<AtomicBool>,

    textures: SyncMutex<Vec<ManagedTexturePtr>>,
    dc_key: u64,

    rect: PropertyRect,
    uv: PropertyRect,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    path: PropertyStr,
    len: PropertyUint32,

    parent_rect: SyncMutex<Option<Rectangle>>,
}

impl Video {
    pub async fn new(node: SceneNodeWeak, render_api: RenderApi) -> Pimpl {
        t!("Video::new()");

        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let uv = PropertyRect::wrap(node_ref, Role::Internal, "uv").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let path = PropertyStr::wrap(node_ref, Role::Internal, "path", 0).unwrap();
        let len = PropertyUint32::wrap(node_ref, Role::Internal, "length", 0).unwrap();

        let self_ = Arc::new(Self {
            node,
            render_api,
            tasks: SyncMutex::new(vec![]),
            stop_load: Arc::new(AtomicBool::new(false)),

            textures: SyncMutex::new(vec![]),
            dc_key: OsRng.gen(),

            rect,
            uv,
            z_index,
            priority,
            path,
            len,

            parent_rect: SyncMutex::new(None),
        });

        Pimpl::Video(self_)
    }

    async fn reload(self: Arc<Self>, batch: BatchGuardPtr) {
        self.load_textures();
        self.clone().redraw(batch).await;
    }

    fn load_textures(&self) {
        let (sendr, recvr) = async_channel::bounded(1);
        let len = self.len.get();
        let path_fmt = self.path.get();
        let render_api = self.render_api.clone();
        let stop_load = self.stop_load.clone();
        let handle = std::thread::spawn(move || {
            let mut textures = Vec::with_capacity(len as usize);
            let instant = std::time::Instant::now();
            for i in 0..len {
                // Stop loading instantly
                if stop_load.load(Ordering::Relaxed) {
                    return
                }
                t!("i = {i}");
                let path = path_fmt.replace("{frame}", &format!("{i:#03}"));
                let texture = Self::load_texture(path, &render_api);
                textures.push(texture);
            }
            t!("elapsed = {:?}", instant.elapsed());
            sendr.send_blocking(textures).unwrap();
        });

        // Temp here
        let Ok(textures) = recvr.recv_blocking() else {
            let node_ref = &self.node.upgrade().unwrap();
            t!("loading textures was stopped {node_ref:?}");
            return
        };
        assert!(handle.is_finished());

        *self.textures.lock() = textures;
    }

    fn load_texture(path: String, render_api: &RenderApi) -> ManagedTexturePtr {
        // TODO we should NOT use panic here
        let data = Arc::new(SyncMutex::new(vec![]));
        let data2 = data.clone();
        miniquad::fs::load_file(&path.clone(), move |res| match res {
            Ok(res) => *data2.lock() = res,
            Err(e) => {
                error!(target: "ui::video", "Unable to open video: {path}: {e}");
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

        render_api.new_texture(width, height, bmp, gfxtag!("img"))
    }

    async fn redraw(self: Arc<Self>, batch: BatchGuardPtr) {
        let trace: DrawTrace = rand::random();
        let timest = unixtime();
        t!("redraw({:?}) [trace={trace}]", self.node.upgrade().unwrap());
        let Some(parent_rect) = self.parent_rect.lock().clone() else { return };

        let atom = &mut batch.spawn();
        let Some(draw_update) = self.get_draw_calls(atom, parent_rect).await else {
            error!(target: "ui::video", "Video failed to draw");
            return
        };
        self.render_api.replace_draw_calls(batch.id, timest, draw_update.draw_calls);
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

    async fn get_draw_calls(
        &self,
        atom: &mut PropertyAtomicGuard,
        parent_rect: Rectangle,
    ) -> Option<DrawUpdate> {
        self.rect.eval(atom, &parent_rect).ok()?;
        let rect = self.rect.get();
        self.uv.eval(atom, &rect).ok()?;

        let mesh = self.regen_mesh();
        let textures = self.textures.lock().clone();
        assert!(!textures.is_empty());

        let mut frames = Vec::with_capacity(textures.len());
        for texture in textures {
            let mesh = DrawMesh {
                vertex_buffer: mesh.vertex_buffer.clone(),
                index_buffer: mesh.index_buffer.clone(),
                texture: Some(texture),
                num_elements: mesh.num_elements,
            };
            let dc = DrawCall {
                instrs: vec![DrawInstruction::Draw(mesh)],
                dcs: vec![],
                z_index: 0,
                debug_str: "video",
            };
            frames.push(SequenceAnimationFrame::new(40, dc));
        }
        let anim = SequenceAnimation::new(false, frames);

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                DrawCall::new(
                    vec![DrawInstruction::Move(rect.pos()), DrawInstruction::Animation(anim)],
                    vec![],
                    self.z_index.get(),
                    "vid",
                ),
            )],
        })
    }
}

#[async_trait]
impl UIObject for Video {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    fn init(&self) {
        self.load_textures();
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
        self.textures.lock().clear();
    }

    async fn draw(
        &self,
        parent_rect: Rectangle,
        trace: DrawTrace,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        t!("Video::draw() [trace={trace}]");
        *self.parent_rect.lock() = Some(parent_rect);
        self.get_draw_calls(atom, parent_rect).await
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        let atom = self.render_api.make_guard(gfxtag!("Video::drop"));
        self.render_api.replace_draw_calls(
            atom.batch_id,
            unixtime(),
            vec![(self.dc_key, Default::default())],
        );
    }
}
