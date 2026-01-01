/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::sync::Arc;
use tracing::instrument;

use crate::{
    gfx::{
        anim::Frame, gfxtag, DrawCall, DrawInstruction, DrawMesh, GraphicPipeline,
        ManagedSeqAnimPtr, ManagedTexturePtr, Rectangle, RenderApi,
    },
    mesh::{MeshBuilder, MeshInfo, COLOR_WHITE},
    prop::{BatchGuardPtr, PropertyAtomicGuard, PropertyRect, PropertyStr, PropertyUint32, Role},
    scene::{Pimpl, SceneNodeWeak},
    ExecutorPtr,
};

use super::{DrawUpdate, OnModify, UIObject};

mod decode;
mod ivf;

use decode::spawn_decoder_thread;

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui:video", $($arg)*); } }

pub type VideoPtr = Arc<Video>;

#[derive(Clone)]
pub struct YuvTextures {
    y: ManagedTexturePtr,
    u: ManagedTexturePtr,
    v: ManagedTexturePtr,
}

#[derive(Clone)]
pub struct Av1VideoData {
    textures: Vec<Option<YuvTextures>>,
    anim: ManagedSeqAnimPtr,

    textures_pub: async_broadcast::Sender<(usize, YuvTextures)>,
    textures_sub: async_broadcast::Receiver<(usize, YuvTextures)>,
}

impl Av1VideoData {
    fn new(len: usize, render_api: &RenderApi) -> Self {
        let (textures_pub, textures_sub) = async_broadcast::broadcast(len);

        let anim = render_api.new_anim(len, false, gfxtag!("video"));
        Self { textures: vec![None; len], anim, textures_pub, textures_sub }
    }
}

pub struct Video {
    node: SceneNodeWeak,
    render_api: RenderApi,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    load_tasks: SyncMutex<Vec<smol::Task<()>>>,
    ex: ExecutorPtr,
    dc_key: u64,

    vid_data: Arc<SyncMutex<Option<Av1VideoData>>>,
    _load_handle: SyncMutex<Option<std::thread::JoinHandle<()>>>,
    _decoder_handle: SyncMutex<Option<std::thread::JoinHandle<()>>>,

    rect: PropertyRect,
    uv: PropertyRect,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    path: PropertyStr,

    parent_rect: SyncMutex<Option<Rectangle>>,
}

impl Video {
    pub async fn new(node: SceneNodeWeak, render_api: RenderApi, ex: ExecutorPtr) -> Pimpl {
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
            load_tasks: SyncMutex::new(vec![]),
            ex,
            dc_key: OsRng.gen(),

            vid_data: Arc::new(SyncMutex::new(None)),
            _load_handle: SyncMutex::new(None),
            _decoder_handle: SyncMutex::new(None),

            rect,
            uv,
            z_index,
            priority,
            path,

            parent_rect: SyncMutex::new(None),
        });

        Pimpl::Video(self_)
    }

    async fn reload(self: Arc<Self>, batch: BatchGuardPtr) {
        self.load_video();
        self.redraw(batch).await;
    }

    fn load_video(&self) {
        let path = self.path.get();

        // Decoder thread:
        // loads path, decodes AV1 -> RGB, creates textures directly
        let decoder_handle =
            spawn_decoder_thread(path, self.vid_data.clone(), self.render_api.clone());

        *self._decoder_handle.lock() = Some(decoder_handle);
    }

    #[instrument(target = "ui::video")]
    async fn redraw(self: Arc<Self>, batch: BatchGuardPtr) {
        let Some(parent_rect) = self.parent_rect.lock().clone() else { return };

        let atom = &mut batch.spawn();
        let Some(draw_update) = self.get_draw_calls(atom, parent_rect).await else {
            error!(target: "ui:video", "Video failed to draw");
            return
        };
        self.render_api.replace_draw_calls(batch.id, draw_update.draw_calls);
    }

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

        let (vid_data, tsubs) = {
            let vid_data_lock = self.vid_data.lock();
            let Some(vid_data) = vid_data_lock.as_ref() else {
                // Video not loaded yet, skip draw
                return None;
            };
            let tsubs = vec![vid_data.textures_sub.clone(); vid_data.textures.len()];
            // Clone the data before the lock is released
            (vid_data.clone(), tsubs)
        };
        assert_eq!(vid_data.textures.len(), tsubs.len());

        let mut load_tasks = self.load_tasks.lock();
        load_tasks.clear();

        let mut loaded_n_frames = 0;
        let total_frames = vid_data.textures.len();

        for (tex_idx, (mut tex, mut tsub)) in
            vid_data.textures.into_iter().zip(tsubs.into_iter()).enumerate()
        {
            let vertex_buffer = mesh.vertex_buffer.clone();
            let index_buffer = mesh.index_buffer.clone();

            let Some(tex) = tex.take() else {
                let anim = vid_data.anim.clone();
                let task = self.ex.spawn(async move {
                    while let Ok((frame_idx, tex)) = tsub.recv().await {
                        if frame_idx != tex_idx {
                            continue
                        }

                        let mesh = DrawMesh {
                            vertex_buffer,
                            index_buffer,
                            textures: Some(vec![tex.y, tex.u, tex.v]),
                            num_elements: mesh.num_elements,
                        };
                        let dc = DrawCall {
                            instrs: vec![DrawInstruction::Draw(mesh)],
                            dcs: vec![],
                            z_index: 0,
                            debug_str: "video",
                        };

                        anim.update(frame_idx, Frame::new(40, dc));
                        break
                    }
                });
                load_tasks.push(task);

                continue
            };
            let mesh = DrawMesh {
                vertex_buffer,
                index_buffer,
                textures: Some(vec![tex.y, tex.u, tex.v]),
                num_elements: mesh.num_elements,
            };
            let dc = DrawCall {
                instrs: vec![DrawInstruction::Draw(mesh)],
                dcs: vec![],
                z_index: 0,
                debug_str: "video",
            };
            vid_data.anim.update(tex_idx, Frame::new(40, dc));
            loaded_n_frames += 1;
        }

        debug!(target: "ui::video", "Loaded {loaded_n_frames} / {total_frames} frames");

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                DrawCall::new(
                    vec![
                        DrawInstruction::SetPipeline(GraphicPipeline::YUV),
                        DrawInstruction::Move(rect.pos()),
                        DrawInstruction::Animation(vid_data.anim.id),
                    ],
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
        self.load_video();
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
        *self.vid_data.lock() = None;
        // Threads terminate naturally when channels close
    }

    #[instrument(target = "ui::video")]
    async fn draw(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        *self.parent_rect.lock() = Some(parent_rect);
        self.get_draw_calls(atom, parent_rect).await
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        let atom = self.render_api.make_guard(gfxtag!("Video::drop"));
        self.render_api.replace_draw_calls(atom.batch_id, vec![(self.dc_key, Default::default())]);
    }
}

impl std::fmt::Debug for Video {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.node.upgrade().unwrap())
    }
}
