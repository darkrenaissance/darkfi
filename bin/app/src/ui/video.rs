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
use parking_lot::{Mutex as SyncMutex, RwLock};
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
        anim::Frame, gfxtag, DrawCall, DrawInstruction, DrawMesh, ManagedSeqAnimPtr,
        ManagedTexturePtr, Rectangle, RenderApi,
    },
    mesh::{MeshBuilder, MeshInfo, COLOR_WHITE},
    prop::{BatchGuardPtr, PropertyAtomicGuard, PropertyRect, PropertyStr, PropertyUint32, Role},
    scene::{Pimpl, SceneNodeWeak},
    util::unixtime,
    ExecutorPtr,
};

use super::{DrawTrace, DrawUpdate, OnModify, UIObject};

pub const N_LOADERS: usize = 4;

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::video", $($arg)*); } }

pub type VideoPtr = Arc<Video>;

#[derive(Clone)]
struct StreamedVideoData {
    textures: Vec<Option<ManagedTexturePtr>>,
    anim: ManagedSeqAnimPtr,
}

impl StreamedVideoData {
    fn new(len: usize, render_api: &RenderApi) -> Self {
        let anim = render_api.new_anim(len, false, gfxtag!("video"));
        Self { textures: vec![None; len], anim }
    }
}

pub struct Video {
    node: SceneNodeWeak,
    render_api: RenderApi,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    load_tasks: SyncMutex<Vec<smol::Task<()>>>,
    ex: ExecutorPtr,
    stop_load: Arc<AtomicBool>,
    dc_key: u64,

    textures_pub: async_broadcast::Sender<(usize, ManagedTexturePtr)>,
    textures_sub: async_broadcast::Receiver<(usize, ManagedTexturePtr)>,
    vid_data: Arc<SyncMutex<Option<StreamedVideoData>>>,
    // Do we need this?
    _load_handles: SyncMutex<[Option<std::thread::JoinHandle<()>>; N_LOADERS]>,

    rect: PropertyRect,
    uv: PropertyRect,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    path: PropertyStr,
    vid_len: PropertyUint32,

    parent_rect: SyncMutex<Option<Rectangle>>,
}

impl Video {
    pub async fn new(node: SceneNodeWeak, render_api: RenderApi, ex: ExecutorPtr) -> Pimpl {
        t!("Video::new()");

        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let uv = PropertyRect::wrap(node_ref, Role::Internal, "uv").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let path = PropertyStr::wrap(node_ref, Role::Internal, "path", 0).unwrap();
        let vid_len = PropertyUint32::wrap(node_ref, Role::Internal, "length", 0).unwrap();

        let (textures_pub, textures_sub) = async_broadcast::broadcast(1);

        let self_ = Arc::new(Self {
            node,
            render_api,
            tasks: SyncMutex::new(vec![]),
            load_tasks: SyncMutex::new(vec![]),
            ex,
            stop_load: Arc::new(AtomicBool::new(false)),
            dc_key: OsRng.gen(),

            textures_pub,
            textures_sub,
            vid_data: Arc::new(SyncMutex::new(None)),
            _load_handles: SyncMutex::new([const { None }; 4]),

            rect,
            uv,
            z_index,
            priority,
            path,
            vid_len,

            parent_rect: SyncMutex::new(None),
        });

        Pimpl::Video(self_)
    }

    async fn reload(self: Arc<Self>, batch: BatchGuardPtr) {
        self.load_textures();
        self.clone().redraw(batch).await;
    }

    fn load_textures(&self) {
        let vid_len = self.vid_len.get() as usize;
        let path_fmt = self.path.get();

        // Starts N threads
        // batch idxs across threads
        //    0    1    2    3
        //    4    5    6    7
        //           ...
        // load_texture:
        //    read image
        //    create texture
        //    set texture slot
        //    broadcast (idx, texture)
        //
        // draw_call:
        //    create broadcast sub
        //    load as many slots from mutex, then release
        //    start task:
        //        loop:
        //            listen to broadcast
        //            create draw_call
        //            send to gfx

        {
            let mut vid_data = self.vid_data.lock();
            *vid_data = Some(StreamedVideoData::new(vid_len, &self.render_api));
            self.textures_pub.clone().set_capacity(vid_len);
        }

        let mut handles = [const { None }; 4];
        for thread_idx in 0..N_LOADERS {
            let path_fmt = path_fmt.clone();
            let render_api = self.render_api.clone();
            let stop_load = self.stop_load.clone();
            let vid_data = self.vid_data.clone();
            let textures_pub = self.textures_pub.clone();

            let handle = std::thread::spawn(move || {
                let mut frame_idx = thread_idx;
                while frame_idx < vid_len {
                    // Stop loading instantly
                    if stop_load.load(Ordering::Relaxed) {
                        return
                    }
                    //t!("frame_idx = {frame_idx} [thread={thread_idx}]");
                    let path = path_fmt.replace("{frame}", &format!("{frame_idx:#03}"));
                    let texture = Self::load_texture(path, &render_api);
                    // Make editing textures array and broadcasting an atomic op
                    {
                        let mut vid_data = vid_data.lock();
                        // panic here on unwrap None when closing app
                        let mut vid_data = vid_data.as_mut().unwrap();
                        vid_data.textures[frame_idx] = Some(texture.clone());
                        // broadcast
                        textures_pub.try_broadcast((frame_idx, texture)).unwrap();
                    }

                    frame_idx += N_LOADERS;
                }
            });
            handles[thread_idx] = Some(handle);
        }
        *self._load_handles.lock() = handles;
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
        // Begin subscribing before we clone Mutex, but actually
        // we need the length of the textures stored, so lets just hold it,
        // do the clones THEN release.
        let (vid_data, tsubs) = {
            let vid_data = self.vid_data.lock();
            let vid_data = vid_data.clone().unwrap();
            // Possibly triggered by race condition.
            // Check it anyway since generally should work.
            assert_eq!(vid_data.textures.len(), self.vid_len.get() as usize);
            let tsubs = vec![self.textures_sub.clone(); vid_data.textures.len()];
            (vid_data, tsubs)
        };
        assert_eq!(vid_data.textures.len(), tsubs.len());

        // Only used in this function so fine to hold the entire time
        let mut load_tasks = self.load_tasks.lock();
        load_tasks.clear();

        for (texture_idx, (mut texture, mut tsub)) in
            vid_data.textures.into_iter().zip(tsubs.into_iter()).enumerate()
        {
            let vertex_buffer = mesh.vertex_buffer.clone();
            let index_buffer = mesh.index_buffer.clone();

            let Some(texture) = texture.take() else {
                let anim = vid_data.anim.clone();
                let task = self.ex.spawn(async move {
                    while let Ok((frame_idx, texture)) = tsub.recv().await {
                        if frame_idx != texture_idx {
                            continue
                        }

                        let mesh = DrawMesh {
                            vertex_buffer,
                            index_buffer,
                            texture: Some(texture),
                            num_elements: mesh.num_elements,
                        };
                        let dc = DrawCall {
                            instrs: vec![DrawInstruction::Draw(mesh)],
                            dcs: vec![],
                            z_index: 0,
                            debug_str: "video",
                        };

                        //t!("sending {frame_idx}");
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
                texture: Some(texture),
                num_elements: mesh.num_elements,
            };
            let dc = DrawCall {
                instrs: vec![DrawInstruction::Draw(mesh)],
                dcs: vec![],
                z_index: 0,
                debug_str: "video",
            };
            vid_data.anim.update(texture_idx, Frame::new(40, dc));
        }

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                DrawCall::new(
                    vec![
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
        *self.vid_data.lock() = None;
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
