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

use std::{
    cell::{Cell, UnsafeCell},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
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
    renderer: Renderer,
    pub(super) tag: DebugTag,
}

impl Drop for ManagedTexture {
    fn drop(&mut self) {
        self.renderer.delete_unmanaged_texture(self.id, self.epoch, self.tag);
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
    renderer: Renderer,
    pub(super) tag: DebugTag,
    pub(super) buftype: u8,
}

impl Drop for ManagedBuffer {
    fn drop(&mut self) {
        self.renderer.delete_unmanaged_buffer(self.id, self.epoch, self.tag, self.buftype);
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
    renderer: Renderer,
    tag: DebugTag,
}

impl ManagedSeqAnim {
    pub fn update(&self, frame_idx: usize, frame: AnimFrame) {
        assert!(frame_idx < self.frames_len);
        self.renderer.update_unmanaged_anim(self.id, frame_idx, frame, self.epoch, self.tag);
    }
}

impl Drop for ManagedSeqAnim {
    fn drop(&mut self) {
        self.renderer.delete_unmanaged_anim(self.id, self.epoch, self.tag);
    }
}

impl std::fmt::Debug for ManagedSeqAnim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedSeqAnim").field("id", &self.id).finish()
    }
}

/// This trait allows `Renderer` and `RendererSync` to be used interchangeably helping with
/// code reuse.
pub trait RenderApi {
    /// Allocate a texture on the gfx card
    fn new_texture(
        &self,
        width: u16,
        height: u16,
        data: Vec<u8>,
        fmt: TextureFormat,
        tag: DebugTag,
    ) -> ManagedTexturePtr;

    /// Create a buffer to store vertices
    fn new_vertex_buffer(&self, verts: Vec<Vertex>, tag: DebugTag) -> ManagedBufferPtr;

    /// Create a buffer to store triangle faces
    fn new_index_buffer(&self, indices: Vec<u16>, tag: DebugTag) -> ManagedBufferPtr;

    /// Modify render tree.
    fn replace_draw_calls(&self, batch_id: Option<BatchGuardId>, dcs: Vec<(DcId, DrawCall)>);
}

#[derive(Clone)]
pub struct Renderer {
    /// We are abusing async_channel since it's cloneable whereas std::sync::mpsc is shit.
    method_send: async_channel::Sender<(EpochIndex, GraphicsMethod)>,
    /// Keep track of the current UI epoch
    epoch: Arc<AtomicU32>,
}

impl Renderer {
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
        Arc::new(ManagedSeqAnim { frames_len, id, epoch, renderer: self.clone(), tag })
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

impl RenderApi for Renderer {
    fn new_texture(
        &self,
        width: u16,
        height: u16,
        data: Vec<u8>,
        fmt: TextureFormat,
        tag: DebugTag,
    ) -> ManagedTexturePtr {
        let (id, epoch) = self.new_unmanaged_texture(width, height, data, fmt, tag);
        Arc::new(ManagedTexture { id, epoch, renderer: self.clone(), tag })
    }

    fn new_vertex_buffer(&self, verts: Vec<Vertex>, tag: DebugTag) -> ManagedBufferPtr {
        let (id, epoch) = self.new_unmanaged_vertex_buffer(verts, tag);
        Arc::new(ManagedBuffer { id, epoch, renderer: self.clone(), tag, buftype: 0 })
    }
    fn new_index_buffer(&self, indices: Vec<u16>, tag: DebugTag) -> ManagedBufferPtr {
        let (id, epoch) = self.new_unmanaged_index_buffer(indices, tag);
        Arc::new(ManagedBuffer { id, epoch, renderer: self.clone(), tag, buftype: 1 })
    }

    fn replace_draw_calls(&self, batch_id: Option<BatchGuardId>, dcs: Vec<(DcId, DrawCall)>) {
        let method = GraphicsMethod::ReplaceGfxDrawCalls { batch_id, dcs };
        self.send(method);

        // I'm not sure whether we need this. Anyway its not fully reliable either since
        // we have no guarantee that when `Stage::update()` whether this method is ready
        // in the receiver.
        #[cfg(target_os = "android")]
        miniquad::window::schedule_update();
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

pub struct RendererSync<'a> {
    stage: UnsafeCell<&'a mut Stage>,
}

impl<'a> RendererSync<'a> {
    pub(super) fn new(stage: &'a mut Stage) -> Self {
        Self { stage: UnsafeCell::new(stage) }
    }

    // SAFETY: The '&a mut Stage' reference stored in UnsafeCell is guaranteed to be unique
    // for the lifetime 'a of this RendererSync. No other references to this Stage can exist
    // while RendererSync is alive, due to Rust's borrow checker rules on mutable references.
    // Therefore, it's safe to return &mut Stage through &self.
    fn stage(&self) -> &mut Stage {
        unsafe { &mut *self.stage.get() }
    }
}

impl RenderApi for RendererSync<'_> {
    fn new_texture(
        &self,
        width: u16,
        height: u16,
        data: Vec<u8>,
        fmt: TextureFormat,
        tag: DebugTag,
    ) -> ManagedTexturePtr {
        let stage = self.stage();
        let renderer = stage.renderer.clone();
        let id = NEXT_TEXTURE_ID.fetch_add(1, Ordering::Relaxed);
        stage.method_new_texture(width, height, &data, fmt, id);
        Arc::new(ManagedTexture { id, epoch: 0, renderer, tag })
    }

    fn new_vertex_buffer(&self, verts: Vec<Vertex>, tag: DebugTag) -> ManagedBufferPtr {
        let stage = self.stage();
        let renderer = stage.renderer.clone();
        let id = NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed);
        stage.method_new_vertex_buffer(&verts, id);
        Arc::new(ManagedBuffer { id, epoch: 0, renderer, tag, buftype: 0 })
    }

    fn new_index_buffer(&self, indices: Vec<u16>, tag: DebugTag) -> ManagedBufferPtr {
        let stage = self.stage();
        let renderer = stage.renderer.clone();
        let id = NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed);
        stage.method_new_index_buffer(&indices, id);
        Arc::new(ManagedBuffer { id, epoch: 0, renderer, tag, buftype: 1 })
    }

    /// Draw calls (no batching). The batch ID is ignored here.
    fn replace_draw_calls(&self, _: Option<BatchGuardId>, dcs: Vec<(DcId, DrawCall)>) {
        let stage = self.stage();
        let timest = unixtime();
        stage.apply_draw_calls(timest, dcs);

        // Force an update
        #[cfg(target_os = "android")]
        miniquad::window::schedule_update();
    }
}
