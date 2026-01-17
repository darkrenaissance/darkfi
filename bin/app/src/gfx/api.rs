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

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use super::{
    anim::Frame as AnimFrame, AnimId, BufferId, DebugTag, DrawCall, Stage, TextureFormat,
    TextureId, Vertex,
};
use crate::{
    prop::{BatchGuardId, PropertyAtomicGuard},
    util::unixtime,
};

pub type EpochIndex = u32;
type DcId = u64;

static NEXT_BUFFER_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_TEXTURE_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_ANIM_ID: AtomicU32 = AtomicU32::new(0);

pub type ManagedTexturePtr = Arc<ManagedTexture>;
pub type ManagedBufferPtr = Arc<ManagedBuffer>;
pub type ManagedSeqAnimPtr = Arc<ManagedSeqAnim>;

/// Auto-deletes texture on drop
pub struct ManagedTexture {
    pub(super) id: TextureId,
    pub(super) epoch: u32,
    render_api: RenderApi,
    pub(super) tag: DebugTag,
}

impl Drop for ManagedTexture {
    fn drop(&mut self) {
        self.render_api.delete_unmanaged_texture(self.id, self.epoch, self.tag);
    }
}

impl std::fmt::Debug for ManagedTexture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedTexture").field("id", &self.id).finish()
    }
}

/// Auto-deletes buffer on drop
pub struct ManagedBuffer {
    pub(super) id: BufferId,
    pub(super) epoch: u32,
    render_api: RenderApi,
    pub(super) tag: DebugTag,
    pub(super) buftype: u8,
}

impl Drop for ManagedBuffer {
    fn drop(&mut self) {
        self.render_api.delete_unmanaged_buffer(self.id, self.epoch, self.tag, self.buftype);
    }
}

impl std::fmt::Debug for ManagedBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedBuffer").field("id", &self.id).finish()
    }
}

pub struct ManagedSeqAnim {
    frames_len: usize,
    pub id: AnimId,
    epoch: u32,
    render_api: RenderApi,
    tag: DebugTag,
}

impl ManagedSeqAnim {
    pub fn update(&self, frame_idx: usize, frame: AnimFrame) {
        assert!(frame_idx < self.frames_len);
        self.render_api.update_unmanaged_anim(self.id, frame_idx, frame, self.epoch, self.tag);
    }
}

impl Drop for ManagedSeqAnim {
    fn drop(&mut self) {
        self.render_api.delete_unmanaged_anim(self.id, self.epoch, self.tag);
    }
}

impl std::fmt::Debug for ManagedSeqAnim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedSeqAnim").field("id", &self.id).finish()
    }
}

#[derive(Clone)]
pub struct RenderApi {
    /// We are abusing async_channel since it's cloneable whereas std::sync::mpsc is shit.
    method_send: async_channel::Sender<(EpochIndex, GraphicsMethod)>,
    /// Keep track of the current UI epoch
    epoch: Arc<AtomicU32>,
}

impl RenderApi {
    pub fn new(method_send: async_channel::Sender<(EpochIndex, GraphicsMethod)>) -> Self {
        Self { method_send, epoch: Arc::new(AtomicU32::new(0)) }
    }

    pub(super) fn next_epoch(&self) -> EpochIndex {
        self.epoch.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn send(&self, method: GraphicsMethod) -> EpochIndex {
        let epoch = self.epoch.load(Ordering::Relaxed);
        self.send_with_epoch(method, epoch);
        epoch
    }
    fn send_with_epoch(&self, method: GraphicsMethod, epoch: EpochIndex) {
        let _ = self.method_send.try_send((epoch, method)).unwrap();
    }

    fn new_unmanaged_texture(
        &self,
        width: u16,
        height: u16,
        data: Vec<u8>,
        fmt: TextureFormat,
        tag: DebugTag,
    ) -> (TextureId, EpochIndex) {
        let gfx_texture_id = NEXT_TEXTURE_ID.fetch_add(1, Ordering::Relaxed);

        let method = GraphicsMethod::NewTexture((width, height, data, fmt, gfx_texture_id, tag));
        let epoch = self.send(method);

        (gfx_texture_id, epoch)
    }

    pub fn new_texture(
        &self,
        width: u16,
        height: u16,
        data: Vec<u8>,
        fmt: TextureFormat,
        tag: DebugTag,
    ) -> ManagedTexturePtr {
        let (id, epoch) = self.new_unmanaged_texture(width, height, data, fmt, tag);
        Arc::new(ManagedTexture { id, epoch, render_api: self.clone(), tag })
    }

    fn delete_unmanaged_texture(&self, texture: TextureId, epoch: EpochIndex, tag: DebugTag) {
        let method = GraphicsMethod::DeleteTexture((texture, tag));
        self.send_with_epoch(method, epoch);
    }

    fn new_unmanaged_vertex_buffer(
        &self,
        verts: Vec<Vertex>,
        tag: DebugTag,
    ) -> (BufferId, EpochIndex) {
        let gfx_buffer_id = NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed);

        let method = GraphicsMethod::NewVertexBuffer((verts, gfx_buffer_id, tag));
        let epoch = self.send(method);

        (gfx_buffer_id, epoch)
    }

    fn new_unmanaged_index_buffer(
        &self,
        indices: Vec<u16>,
        tag: DebugTag,
    ) -> (BufferId, EpochIndex) {
        let gfx_buffer_id = NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed);

        let method = GraphicsMethod::NewIndexBuffer((indices, gfx_buffer_id, tag));
        let epoch = self.send(method);

        (gfx_buffer_id, epoch)
    }

    pub fn new_vertex_buffer(&self, verts: Vec<Vertex>, tag: DebugTag) -> ManagedBufferPtr {
        let (id, epoch) = self.new_unmanaged_vertex_buffer(verts, tag);
        Arc::new(ManagedBuffer { id, epoch, render_api: self.clone(), tag, buftype: 0 })
    }
    pub fn new_index_buffer(&self, indices: Vec<u16>, tag: DebugTag) -> ManagedBufferPtr {
        let (id, epoch) = self.new_unmanaged_index_buffer(indices, tag);
        Arc::new(ManagedBuffer { id, epoch, render_api: self.clone(), tag, buftype: 1 })
    }

    fn delete_unmanaged_buffer(
        &self,
        buffer: BufferId,
        epoch: EpochIndex,
        tag: DebugTag,
        buftype: u8,
    ) {
        let method = GraphicsMethod::DeleteBuffer((buffer, tag, buftype));
        self.send_with_epoch(method, epoch);
    }

    fn new_unmanaged_anim(
        &self,
        frames_len: usize,
        oneshot: bool,
        tag: DebugTag,
    ) -> (AnimId, EpochIndex) {
        let gfx_anim_id = NEXT_ANIM_ID.fetch_add(1, Ordering::Relaxed);

        let method = GraphicsMethod::NewSeqAnim { id: gfx_anim_id, frames_len, oneshot, tag };
        let epoch = self.send(method);

        (gfx_anim_id, epoch)
    }

    pub fn new_anim(&self, frames_len: usize, oneshot: bool, tag: DebugTag) -> ManagedSeqAnimPtr {
        let (id, epoch) = self.new_unmanaged_anim(frames_len, oneshot, tag);
        Arc::new(ManagedSeqAnim { frames_len, id, epoch, render_api: self.clone(), tag })
    }

    pub fn update_unmanaged_anim(
        &self,
        anim: AnimId,
        frame_idx: usize,
        frame: AnimFrame,
        epoch: EpochIndex,
        tag: DebugTag,
    ) {
        let method = GraphicsMethod::UpdateSeqAnim { id: anim, frame_idx, frame, tag };
        self.send_with_epoch(method, epoch);
    }

    fn delete_unmanaged_anim(&self, anim: AnimId, epoch: EpochIndex, tag: DebugTag) {
        let method = GraphicsMethod::DeleteSeqAnim((anim, tag));
        self.send_with_epoch(method, epoch);
    }

    pub fn replace_draw_calls(&self, batch_id: Option<BatchGuardId>, dcs: Vec<(DcId, DrawCall)>) {
        let method = GraphicsMethod::ReplaceGfxDrawCalls { batch_id, dcs };
        self.send(method);

        // I'm not sure whether we need this. Anyway its not fully reliable either since
        // we have no guarantee that when `Stage::update()` whether this method is ready
        // in the receiver.
        #[cfg(target_os = "android")]
        miniquad::window::schedule_update();
    }

    fn start_batch(&self, batch_id: BatchGuardId, tag: DebugTag) {
        let method = GraphicsMethod::StartBatch { batch_id, tag };
        self.send(method);
    }
    fn end_batch(&self, batch_id: BatchGuardId) {
        let timest = unixtime();
        let method = GraphicsMethod::EndBatch { batch_id, timest };
        self.send(method);

        // Force an update
        #[cfg(target_os = "android")]
        miniquad::window::schedule_update();
    }

    pub fn make_guard(&self, debug_str: Option<&'static str>) -> PropertyAtomicGuard {
        let r = self.clone();
        let start_batch = Box::new(move |bid| r.start_batch(bid, debug_str));
        let r = self.clone();
        let end_batch = Box::new(move |bid| r.end_batch(bid));
        PropertyAtomicGuard::new(start_batch, end_batch)
    }
}

#[derive(Clone)]
pub enum GraphicsMethod {
    NewTexture((u16, u16, Vec<u8>, TextureFormat, TextureId, DebugTag)),
    DeleteTexture((TextureId, DebugTag)),
    NewVertexBuffer((Vec<Vertex>, BufferId, DebugTag)),
    NewIndexBuffer((Vec<u16>, BufferId, DebugTag)),
    DeleteBuffer((BufferId, DebugTag, u8)),
    NewSeqAnim { id: AnimId, frames_len: usize, oneshot: bool, tag: DebugTag },
    UpdateSeqAnim { id: AnimId, frame_idx: usize, frame: AnimFrame, tag: DebugTag },
    DeleteSeqAnim((AnimId, DebugTag)),
    ReplaceGfxDrawCalls { batch_id: Option<BatchGuardId>, dcs: Vec<(DcId, DrawCall)> },
    StartBatch { batch_id: BatchGuardId, tag: DebugTag },
    EndBatch { batch_id: BatchGuardId, timest: u64 },
    Noop,
}

impl std::fmt::Debug for GraphicsMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NewTexture(_) => write!(f, "NewTexture"),
            Self::DeleteTexture(_) => write!(f, "DeleteTexture"),
            Self::NewVertexBuffer(_) => write!(f, "NewVertexBuffer"),
            Self::NewIndexBuffer(_) => write!(f, "NewIndexBuffer"),
            Self::DeleteBuffer(_) => write!(f, "DeleteBuffer"),
            Self::NewSeqAnim { .. } => write!(f, "NewSeqAnim"),
            Self::UpdateSeqAnim { .. } => write!(f, "UpdateSeqAnim"),
            Self::DeleteSeqAnim(_) => write!(f, "DeleteSeqAnim"),
            Self::ReplaceGfxDrawCalls { batch_id, dcs: _ } => match batch_id {
                Some(bid) => write!(f, "ReplaceGfxDrawCalls({bid})"),
                None => write!(f, "ReplaceGfxDrawCalls(immediate)"),
            },
            Self::StartBatch { batch_id, tag } => write!(f, "StartBatch({batch_id}, {tag:?})"),
            Self::EndBatch { batch_id, timest } => write!(f, "EndBatch({batch_id}, {timest})"),
            Self::Noop => write!(f, "Noop"),
        }
    }
}

impl Default for GraphicsMethod {
    fn default() -> Self {
        GraphicsMethod::Noop
    }
}

pub struct RenderApiSync<'a> {
    stage: &'a mut Stage,
    is_dirty: bool,
}

impl<'a> RenderApiSync<'a> {
    pub fn new(stage: &'a mut Stage) -> Self {
        Self { stage, is_dirty: false }
    }

    // Texture methods
    pub fn new_texture(
        &mut self,
        width: u16,
        height: u16,
        data: Vec<u8>,
        fmt: TextureFormat,
        tag: DebugTag,
    ) -> ManagedTexturePtr {
        let render_api = self.stage.render_api.clone();
        let id = NEXT_TEXTURE_ID.fetch_add(1, Ordering::Relaxed);
        self.stage.method_new_texture(width, height, &data, fmt, id);
        Arc::new(ManagedTexture { id, epoch: 0, render_api, tag })
    }

    // Buffer methods
    pub fn new_vertex_buffer(&mut self, verts: Vec<Vertex>, tag: DebugTag) -> ManagedBufferPtr {
        let render_api = self.stage.render_api.clone();
        let id = NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed);
        self.stage.method_new_vertex_buffer(&verts, id);
        Arc::new(ManagedBuffer { id, epoch: 0, render_api, tag, buftype: 0 })
    }

    pub fn new_index_buffer(&mut self, indices: Vec<u16>, tag: DebugTag) -> ManagedBufferPtr {
        let render_api = self.stage.render_api.clone();
        let id = NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed);
        self.stage.method_new_index_buffer(&indices, id);
        Arc::new(ManagedBuffer { id, epoch: 0, render_api, tag, buftype: 1 })
    }

    // Draw calls (no batching)
    pub fn replace_draw_calls(&mut self, dcs: Vec<(DcId, DrawCall)>) {
        let timest = unixtime();
        self.stage.apply_draw_calls(timest, dcs);
        self.is_dirty = true;
    }
}

impl Drop for RenderApiSync<'_> {
    fn drop(&mut self) {
        if self.is_dirty {
            // Force an update
            #[cfg(target_os = "android")]
            miniquad::window::schedule_update();
        }
    }
}
