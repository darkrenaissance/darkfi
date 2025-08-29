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

use darkfi_serial::{
    async_trait, AsyncEncodable, AsyncWrite, Decodable, Encodable, FutAsyncWriteExt,
    SerialDecodable, SerialEncodable,
};
use log::debug;
#[cfg(target_os = "android")]
use miniquad::native::egl;
use miniquad::{
    conf, window, Backend, Bindings, BlendFactor, BlendState, BlendValue, BufferLayout,
    BufferSource, BufferType, BufferUsage, Equation, EventHandler, KeyCode, KeyMods, MouseButton,
    PassAction, Pipeline, PipelineParams, RenderingBackend, ShaderMeta, ShaderSource, TouchPhase,
    UniformDesc, UniformType, VertexAttribute, VertexFormat,
};
use parking_lot::Mutex as SyncMutex;
use std::{
    cell::RefCell,
    collections::HashMap,
    fs::File,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

pub mod anim;
use anim::{Frame as AnimFrame, GfxSeqAnim};
mod favico;
mod linalg;
pub use linalg::{Dimension, Point, Rectangle};
mod shader;
mod trax;
use trax::get_trax;

use crate::{
    error::{Error, Result},
    prop::{BatchGuardId, PropertyAtomicGuard},
    ExecutorPtr, GOD,
};

// This is very noisy so suppress output by default
const DEBUG_RENDER: bool = false;
const DEBUG_GFXAPI: bool = false;
const DEBUG_TRAX: bool = false;

#[macro_export]
macro_rules! gfxtag {
    ($s:expr) => {{
        Some($s)
    }};
}
pub use crate::gfxtag;

pub type DebugTag = Option<&'static str>;

macro_rules! t { ($($arg:tt)*) => { trace!(target: "gfx", $($arg)*); } }
macro_rules! e { ($($arg:tt)*) => { error!(target: "gfx", $($arg)*); } }

#[cfg(target_os = "android")]
pub fn get_window_size_filename() -> PathBuf {
    crate::android::get_appdata_path().join("window_size")
}
#[cfg(not(target_os = "android"))]
pub fn get_window_size_filename() -> PathBuf {
    dirs::cache_dir().unwrap().join("darkfi/app/window_size")
}

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
#[repr(C)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

impl Vertex {
    pub fn pos(&self) -> Point {
        self.pos.into()
    }
    pub fn set_pos(&mut self, pos: &Point) {
        self.pos = pos.as_arr();
    }
}

pub type TextureId = u32;
pub type BufferId = u32;
pub type AnimId = u32;

static NEXT_BUFFER_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_TEXTURE_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_ANIM_ID: AtomicU32 = AtomicU32::new(0);

pub type ManagedTexturePtr = Arc<ManagedTexture>;

/// Auto-deletes texture on drop
pub struct ManagedTexture {
    id: TextureId,
    epoch: u32,
    render_api: RenderApi,
    tag: DebugTag,
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

pub type ManagedBufferPtr = Arc<ManagedBuffer>;

/// Auto-deletes buffer on drop
pub struct ManagedBuffer {
    id: BufferId,
    epoch: u32,
    render_api: RenderApi,
    tag: DebugTag,
    buftype: u8,
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

pub type ManagedSeqAnimPtr = Arc<ManagedSeqAnim>;

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

pub type EpochIndex = u32;

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

    fn next_epoch(&self) -> EpochIndex {
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
        tag: DebugTag,
    ) -> (TextureId, EpochIndex) {
        let gfx_texture_id = NEXT_TEXTURE_ID.fetch_add(1, Ordering::Relaxed);

        let method = GraphicsMethod::NewTexture((width, height, data, gfx_texture_id, tag));
        let epoch = self.send(method);

        (gfx_texture_id, epoch)
    }

    pub fn new_texture(
        &self,
        width: u16,
        height: u16,
        data: Vec<u8>,
        tag: DebugTag,
    ) -> ManagedTexturePtr {
        let (id, epoch) = self.new_unmanaged_texture(width, height, data, tag);
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

    pub fn replace_draw_calls(
        &self,
        batch_id: BatchGuardId,
        timest: Timestamp,
        dcs: Vec<(DcId, DrawCall)>,
    ) {
        let method = GraphicsMethod::ReplaceGfxDrawCalls { batch_id, timest, dcs };
        self.send(method);
    }

    fn start_batch(&self, batch_id: BatchGuardId, debug_str: Option<&'static str>) {
        let method = GraphicsMethod::StartBatch((batch_id, debug_str));
        self.send(method);
    }
    fn end_batch(&self, batch_id: BatchGuardId) {
        let method = GraphicsMethod::EndBatch(batch_id);
        self.send(method);
    }

    pub fn make_guard(&self, debug_str: Option<&'static str>) -> PropertyAtomicGuard {
        let r = self.clone();
        let start_batch = Box::new(move |bid| r.start_batch(bid, debug_str));
        let r = self.clone();
        let end_batch = Box::new(move |bid| r.end_batch(bid));
        PropertyAtomicGuard::new(start_batch, end_batch)
    }
}

#[derive(Clone, Debug)]
pub struct DrawMesh {
    pub vertex_buffer: ManagedBufferPtr,
    pub index_buffer: ManagedBufferPtr,
    pub texture: Option<ManagedTexturePtr>,
    pub num_elements: i32,
}

impl DrawMesh {
    fn compile(
        self,
        textures: &HashMap<TextureId, miniquad::TextureId>,
        buffers: &HashMap<BufferId, miniquad::BufferId>,
        debug_str: &'static str,
    ) -> Option<GfxDrawMesh> {
        let vertex_buffer_id = self.vertex_buffer.id;
        let index_buffer_id = self.index_buffer.id;
        let _buffers_keep_alive = [self.vertex_buffer, self.index_buffer];
        let texture = match self.texture {
            Some(gfx_texture) => Self::try_get_texture(textures, gfx_texture, debug_str),
            None => None,
        };
        Some(GfxDrawMesh {
            vertex_buffer: Self::try_get_buffer(buffers, vertex_buffer_id, debug_str)?,
            index_buffer: Self::try_get_buffer(buffers, index_buffer_id, debug_str)?,
            _buffers_keep_alive,
            texture,
            num_elements: self.num_elements,
        })
    }

    fn try_get_texture(
        textures: &HashMap<TextureId, miniquad::TextureId>,
        gfx_texture: ManagedTexturePtr,
        debug_str: &'static str,
    ) -> Option<(ManagedTexturePtr, miniquad::TextureId)> {
        let gfx_texture_id = gfx_texture.id;

        let Some(_mq_texture_id) = textures.get(&gfx_texture_id) else {
            error!(target: "gfx", "Serious error: missing texture ID={gfx_texture_id}, debug={debug_str}");
            error!(target: "gfx", "Dumping textures:");
            for (gfx_texture_id, texture_id) in textures {
                error!(target: "gfx", "{gfx_texture_id} => {texture_id:?}");
            }

            panic!("Missing texture ID={gfx_texture_id}")
        };

        Some((gfx_texture, textures[&gfx_texture_id]))
    }

    fn try_get_buffer(
        buffers: &HashMap<BufferId, miniquad::BufferId>,
        gfx_buffer_id: BufferId,
        debug_str: &'static str,
    ) -> Option<miniquad::BufferId> {
        let Some(mq_buffer_id) = buffers.get(&gfx_buffer_id) else {
            error!(target: "gfx", "Serious error: missing buffer ID={gfx_buffer_id}, debug={debug_str}");
            error!(target: "gfx", "Dumping buffers:");
            for (gfx_buffer_id, buffer_id) in buffers {
                error!(target: "gfx", "{gfx_buffer_id} => {buffer_id:?}");
            }

            panic!("Missing buffer ID={gfx_buffer_id}")
        };
        Some(*mq_buffer_id)
    }
}

impl Encodable for DrawMesh {
    fn encode<S: Write>(&self, s: &mut S) -> std::result::Result<usize, std::io::Error> {
        let mut len = 0;
        len += self.vertex_buffer.id.encode(s)?;
        len += self.vertex_buffer.epoch.encode(s)?;
        len += self.vertex_buffer.tag.encode(s)?;
        len += self.vertex_buffer.buftype.encode(s)?;
        len += self.index_buffer.id.encode(s)?;
        len += self.index_buffer.epoch.encode(s)?;
        len += self.index_buffer.tag.encode(s)?;
        len += self.index_buffer.buftype.encode(s)?;
        match &self.texture {
            Some(t) => {
                len += 1u8.encode(s)?;
                len += t.id.encode(s)?;
                len += t.epoch.encode(s)?;
                len += t.tag.encode(s)?;
            }
            None => {
                len += 0u8.encode(s)?;
            }
        }
        len += self.num_elements.encode(s)?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncEncodable for DrawMesh {
    async fn encode_async<W: AsyncWrite + Unpin + Send>(
        &self,
        _: &mut W,
    ) -> std::io::Result<usize> {
        Ok(0)
    }
}

#[derive(Debug, Clone, SerialEncodable)]
pub enum DrawInstruction {
    SetScale(f32),
    Move(Point),
    SetPos(Point),
    ApplyView(Rectangle),
    Draw(DrawMesh),
    Animation(AnimId),
    EnableDebug,
}

impl DrawInstruction {
    fn compile(
        self,
        textures: &HashMap<TextureId, miniquad::TextureId>,
        buffers: &HashMap<BufferId, miniquad::BufferId>,
        debug_str: &'static str,
    ) -> Option<GfxDrawInstruction> {
        let instr = match self {
            Self::SetScale(scale) => GfxDrawInstruction::SetScale(scale),
            Self::Move(off) => GfxDrawInstruction::Move(off),
            Self::SetPos(pos) => GfxDrawInstruction::SetPos(pos),
            Self::ApplyView(view) => GfxDrawInstruction::ApplyView(view),
            Self::Draw(mesh) => {
                GfxDrawInstruction::Draw(mesh.compile(textures, buffers, debug_str)?)
            }
            Self::Animation(anim) => GfxDrawInstruction::Animation(anim),
            Self::EnableDebug => GfxDrawInstruction::EnableDebug,
        };
        Some(instr)
    }
}

#[derive(Clone, Debug, Default, SerialEncodable)]
pub struct DrawCall {
    pub instrs: Vec<DrawInstruction>,
    pub dcs: Vec<DcId>,
    pub z_index: u32,
    pub debug_str: &'static str,
}

impl DrawCall {
    pub fn new(
        instrs: Vec<DrawInstruction>,
        dcs: Vec<DcId>,
        z_index: u32,
        debug_str: &'static str,
    ) -> Self {
        Self { instrs, dcs, z_index, debug_str }
    }

    fn compile(
        self,
        textures: &HashMap<TextureId, miniquad::TextureId>,
        buffers: &HashMap<BufferId, miniquad::BufferId>,
        timest: Timestamp,
    ) -> Option<GfxDrawCall> {
        Some(GfxDrawCall {
            instrs: self
                .instrs
                .into_iter()
                .map(|i| i.compile(textures, buffers, self.debug_str))
                .collect::<Option<Vec<_>>>()?,
            dcs: self.dcs,
            z_index: self.z_index,
            timest,
        })
    }
}

#[derive(Clone, Debug)]
struct GfxDrawMesh {
    vertex_buffer: miniquad::BufferId,
    index_buffer: miniquad::BufferId,
    /// Keeps the buffers alive for the duration of this draw call
    _buffers_keep_alive: [ManagedBufferPtr; 2],
    texture: Option<(ManagedTexturePtr, miniquad::TextureId)>,
    num_elements: i32,
}

#[derive(Debug, Clone)]
enum GfxDrawInstruction {
    SetScale(f32),
    Move(Point),
    SetPos(Point),
    ApplyView(Rectangle),
    Draw(GfxDrawMesh),
    Animation(AnimId),
    EnableDebug,
}

#[derive(Clone, Debug)]
struct GfxDrawCall {
    instrs: Vec<GfxDrawInstruction>,
    dcs: Vec<DcId>,
    z_index: u32,
    timest: Timestamp,
}

struct RenderContext<'a> {
    ctx: &'a mut Box<dyn RenderingBackend>,
    draw_calls: &'a HashMap<DcId, GfxDrawCall>,
    uniforms_data: [u8; 128],
    white_texture: miniquad::TextureId,

    scale: f32,
    view: Rectangle,
    cursor: Point,

    anims: &'a mut HashMap<AnimId, GfxSeqAnim>,
}

impl<'a> RenderContext<'a> {
    fn draw(&mut self) {
        if DEBUG_RENDER {
            debug!(target: "gfx", "RenderContext::draw()");
        }
        if DEBUG_TRAX {
            get_trax().lock().set_curr(0);
        }
        self.draw_call(&self.draw_calls[&0], 0, DEBUG_RENDER);
        if DEBUG_RENDER {
            debug!(target: "gfx", "RenderContext::draw() [DONE]");
        }
    }

    fn apply_view(&mut self) {
        // Actual physical view
        let view = self.view * self.scale;
        let (_, screen_height) = window::screen_size();

        let view_x = view.x.round() as i32;
        let view_y = screen_height - (view.y + view.h);
        let view_y = view_y.round() as i32;
        let view_w = view.w.round() as i32;
        let view_h = view.h.round() as i32;

        // OpenGL does not like negative values here
        if view_w <= 0 || view_h <= 0 {
            return
        }

        if DEBUG_RENDER {
            debug!(target: "gfx", "=> viewport {view_x} {view_y} {view_w} {view_h}");
        }
        self.ctx.apply_viewport(view_x, view_y, view_w, view_h);
        self.ctx.apply_scissor_rect(view_x, view_y, view_w, view_h);
    }

    fn apply_model(&mut self) {
        let off_x = self.cursor.x / self.view.w;
        let off_y = self.cursor.y / self.view.h;

        let scale_w = 1. / self.view.w;
        let scale_h = 1. / self.view.h;

        let model = glam::Mat4::from_translation(glam::Vec3::new(off_x, off_y, 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(scale_w, scale_h, 1.));

        let data: [u8; 64] = unsafe { std::mem::transmute_copy(&model) };
        self.uniforms_data[64..].copy_from_slice(&data);
        self.ctx.apply_uniforms_from_bytes(self.uniforms_data.as_ptr(), self.uniforms_data.len());
    }

    fn draw_call(&mut self, draw_call: &GfxDrawCall, mut indent: u32, mut is_debug: bool) {
        let ws = if is_debug { " ".repeat(indent as usize * 4) } else { String::new() };

        let old_scale = self.scale;
        let old_view = self.view;
        let old_cursor = self.cursor;

        for (idx, instr) in draw_call.instrs.iter().enumerate() {
            if DEBUG_TRAX {
                get_trax().lock().set_instr(idx);
            }
            match instr {
                GfxDrawInstruction::SetScale(scale) => {
                    self.scale = *scale;
                    self.view.w /= self.scale;
                    self.view.h /= self.scale;
                    if is_debug {
                        debug!(target: "gfx", "{ws}set_scale({scale})");
                    }
                }
                GfxDrawInstruction::Move(off) => {
                    self.cursor += *off;
                    if is_debug {
                        debug!(target: "gfx",
                            "{ws}move({off:?})  cursor={:?}, scale={}, view={:?}",
                            self.cursor, self.scale, self.view
                        );
                    }
                    self.apply_model();
                }
                GfxDrawInstruction::SetPos(pos) => {
                    self.cursor = old_cursor + *pos;
                    if is_debug {
                        debug!(target: "gfx",
                            "{ws}set_pos({pos:?})  cursor={:?}, scale={}, view={:?}",
                            self.cursor, self.scale, self.view
                        );
                    }
                    self.apply_model();
                }
                GfxDrawInstruction::ApplyView(view) => {
                    // Adjust view relative to cursor
                    self.view = *view + self.cursor;

                    // We could just skip drawing when clipping rect isn't visible
                    // using an is_visible flag.
                    match self.view.clip(&old_view) {
                        Some(clipped) => self.view = clipped,
                        None => self.view = Rectangle::zero(),
                    }

                    // Cursor resets within the view
                    self.cursor = Point::zero();
                    if is_debug {
                        debug!(target: "gfx",
                            "{ws}apply_view({view:?})  scale={}, view={:?}",
                            self.scale, self.view
                        );
                    }
                    self.apply_view();
                    self.apply_model();
                }
                GfxDrawInstruction::Draw(mesh) => {
                    if is_debug {
                        debug!(target: "gfx", "{ws}draw({mesh:?})");
                    }
                    let texture = match mesh.texture {
                        Some((_, texture)) => texture,
                        None => self.white_texture,
                    };
                    let bindings = Bindings {
                        vertex_buffers: vec![mesh.vertex_buffer],
                        index_buffer: mesh.index_buffer,
                        images: vec![texture],
                    };
                    self.ctx.apply_bindings(&bindings);
                    self.ctx.draw(0, mesh.num_elements, 1);
                }
                GfxDrawInstruction::Animation(anim_id) => {
                    let anim = self.anims.get_mut(&anim_id).unwrap();
                    anim.is_visible = true;
                    if let Some(dc) = anim.tick() {
                        self.draw_call(&dc, indent + 1, is_debug);
                    }
                }
                GfxDrawInstruction::EnableDebug => {
                    if !is_debug {
                        indent = 0;
                    }
                    is_debug = true;
                    debug!(target: "gfx", "Frame start");
                }
            }
        }

        let mut draw_calls: Vec<_> =
            draw_call.dcs.iter().map(|key| (key, &self.draw_calls[key])).collect();
        draw_calls.sort_unstable_by_key(|(_, dc)| dc.z_index);

        for (dc_key, dc) in draw_calls {
            if DEBUG_TRAX {
                get_trax().lock().set_curr(*dc_key);
            }
            if is_debug {
                debug!(target: "gfx", "{ws}drawcall {dc_key}");
            }
            self.draw_call(dc, indent + 1, is_debug);
        }

        self.scale = old_scale;

        if is_debug {
            debug!(target: "gfx", "{ws}Frame close: cursor={old_cursor:?}, view={old_view:?}");
        }

        self.view = old_view;
        self.apply_view();

        self.cursor = old_cursor;
        self.apply_model();
    }
}

type Timestamp = u64;
type DcId = u64;

#[derive(Clone)]
pub enum GraphicsMethod {
    NewTexture((u16, u16, Vec<u8>, TextureId, DebugTag)),
    DeleteTexture((TextureId, DebugTag)),
    NewVertexBuffer((Vec<Vertex>, BufferId, DebugTag)),
    NewIndexBuffer((Vec<u16>, BufferId, DebugTag)),
    DeleteBuffer((BufferId, DebugTag, u8)),
    NewSeqAnim { id: AnimId, frames_len: usize, oneshot: bool, tag: DebugTag },
    UpdateSeqAnim { id: AnimId, frame_idx: usize, frame: AnimFrame, tag: DebugTag },
    DeleteSeqAnim((AnimId, DebugTag)),
    ReplaceGfxDrawCalls { batch_id: BatchGuardId, timest: Timestamp, dcs: Vec<(DcId, DrawCall)> },
    StartBatch((BatchGuardId, Option<&'static str>)),
    EndBatch(BatchGuardId),
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
            Self::ReplaceGfxDrawCalls { batch_id: bid, timest: _, dcs: _ } => {
                write!(f, "ReplaceGfxDrawCalls({bid})")
            }
            Self::StartBatch((bid, debug_str)) => write!(f, "StartBatch({bid}, {debug_str:?})"),
            Self::EndBatch(bid) => write!(f, "EndBatch({bid})"),
        }
    }
}

struct EventChannel<T> {
    sender: async_channel::Sender<T>,
    recvr: async_channel::Receiver<T>,
}

impl<T> EventChannel<T> {
    fn new() -> Self {
        let (sender, recvr) = async_channel::unbounded();
        Self { sender, recvr }
    }

    fn notify(&self, ev: T) {
        self.sender.try_send(ev).unwrap();
    }

    fn clone_recvr(&self) -> async_channel::Receiver<T> {
        self.recvr.clone()
    }
}

pub type GraphicsEventPublisherPtr = Arc<GraphicsEventPublisher>;

pub struct GraphicsEventPublisher {
    resize: EventChannel<Dimension>,
    key_down: EventChannel<(KeyCode, KeyMods, bool)>,
    key_up: EventChannel<(KeyCode, KeyMods)>,
    chr: EventChannel<(char, KeyMods, bool)>,
    mouse_btn_down: EventChannel<(MouseButton, Point)>,
    mouse_btn_up: EventChannel<(MouseButton, Point)>,
    mouse_move: EventChannel<Point>,
    mouse_wheel: EventChannel<Point>,
    touch: EventChannel<(TouchPhase, u64, Point)>,
}

pub type GraphicsEventResizeSub = async_channel::Receiver<Dimension>;
pub type GraphicsEventKeyDownSub = async_channel::Receiver<(KeyCode, KeyMods, bool)>;
pub type GraphicsEventKeyUpSub = async_channel::Receiver<(KeyCode, KeyMods)>;
pub type GraphicsEventCharSub = async_channel::Receiver<(char, KeyMods, bool)>;
pub type GraphicsEventMouseButtonDownSub = async_channel::Receiver<(MouseButton, Point)>;
pub type GraphicsEventMouseButtonUpSub = async_channel::Receiver<(MouseButton, Point)>;
pub type GraphicsEventMouseMoveSub = async_channel::Receiver<Point>;
pub type GraphicsEventMouseWheelSub = async_channel::Receiver<Point>;
pub type GraphicsEventTouchSub = async_channel::Receiver<(TouchPhase, u64, Point)>;

impl GraphicsEventPublisher {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            resize: EventChannel::new(),
            key_down: EventChannel::new(),
            key_up: EventChannel::new(),
            chr: EventChannel::new(),
            mouse_btn_down: EventChannel::new(),
            mouse_btn_up: EventChannel::new(),
            mouse_move: EventChannel::new(),
            mouse_wheel: EventChannel::new(),
            touch: EventChannel::new(),
        })
    }

    fn notify_resize(&self, screen_size: Dimension) {
        self.resize.notify(screen_size);
    }
    fn notify_key_down(&self, key: KeyCode, mods: KeyMods, repeat: bool) {
        let ev = (key, mods, repeat);
        self.key_down.notify(ev);
    }
    fn notify_key_up(&self, key: KeyCode, mods: KeyMods) {
        let ev = (key, mods);
        self.key_up.notify(ev);
    }
    fn notify_char(&self, chr: char, mods: KeyMods, repeat: bool) {
        let ev = (chr, mods, repeat);
        self.chr.notify(ev);
    }
    fn notify_mouse_btn_down(&self, button: MouseButton, mouse_pos: Point) {
        let ev = (button, mouse_pos);
        self.mouse_btn_down.notify(ev);
    }
    fn notify_mouse_btn_up(&self, button: MouseButton, mouse_pos: Point) {
        let ev = (button, mouse_pos);
        self.mouse_btn_up.notify(ev);
    }

    fn notify_mouse_move(&self, mouse_pos: Point) {
        self.mouse_move.notify(mouse_pos);
    }
    fn notify_mouse_wheel(&self, wheel_pos: Point) {
        self.mouse_wheel.notify(wheel_pos);
    }
    fn notify_touch(&self, phase: TouchPhase, id: u64, touch_pos: Point) {
        let ev = (phase, id, touch_pos);
        self.touch.notify(ev);
    }

    pub fn subscribe_resize(&self) -> GraphicsEventResizeSub {
        self.resize.clone_recvr()
    }
    pub fn subscribe_key_down(&self) -> GraphicsEventKeyDownSub {
        self.key_down.clone_recvr()
    }
    pub fn subscribe_key_up(&self) -> GraphicsEventKeyUpSub {
        self.key_up.clone_recvr()
    }
    pub fn subscribe_char(&self) -> GraphicsEventCharSub {
        self.chr.clone_recvr()
    }
    pub fn subscribe_mouse_btn_down(&self) -> GraphicsEventMouseButtonDownSub {
        self.mouse_btn_down.clone_recvr()
    }
    pub fn subscribe_mouse_btn_up(&self) -> GraphicsEventMouseButtonUpSub {
        self.mouse_btn_up.clone_recvr()
    }
    pub fn subscribe_mouse_move(&self) -> GraphicsEventMouseMoveSub {
        self.mouse_move.clone_recvr()
    }
    pub fn subscribe_mouse_wheel(&self) -> GraphicsEventMouseWheelSub {
        self.mouse_wheel.clone_recvr()
    }
    pub fn subscribe_touch(&self) -> GraphicsEventTouchSub {
        self.touch.clone_recvr()
    }
}

struct Stage {
    ctx: Box<dyn RenderingBackend>,
    #[cfg(target_os = "android")]
    libegl: egl::LibEgl,
    pipeline: Pipeline,
    white_texture: miniquad::TextureId,
    draw_calls: HashMap<DcId, GfxDrawCall>,
    batches: HashMap<BatchGuardId, Vec<GraphicsMethod>>,

    textures: HashMap<TextureId, miniquad::TextureId>,
    buffers: HashMap<BufferId, miniquad::BufferId>,
    anims: HashMap<AnimId, GfxSeqAnim>,

    epoch: EpochIndex,
    method_queue: Arc<SyncMutex<Vec<(EpochIndex, GraphicsMethod)>>>,
    event_pub: GraphicsEventPublisherPtr,

    pruner: PruneMethodHeap,
    screen_was_off: bool,
    ex: ExecutorPtr,
    #[cfg(target_os = "android")]
    refresh_task: Option<smol::Task<()>>,
}

impl Stage {
    pub fn new() -> Self {
        if DEBUG_TRAX {
            get_trax().lock().clear();
        }
        let mut ctx: Box<dyn RenderingBackend> = window::new_rendering_backend();

        let god = GOD.get().unwrap();
        // Start a new epoch. This is a brand new UI run.
        let epoch = god.render_api.next_epoch();
        // This will start the app to start. Needed since we cannot get window size for init
        // until window is created.
        god.start_app(epoch);
        let method_recv = god.method_recv.clone();
        let event_pub = god.event_pub.clone();

        let ex = god.fg_ex.clone();
        let method_queue = Arc::new(SyncMutex::new(vec![]));
        let method_queue2 = method_queue.clone();
        let sink_task = ex.spawn(async move {
            // Pull from render_api
            while let Ok((epoch, method)) = method_recv.recv().await {
                let is_replace_dc = matches!(method, GraphicsMethod::ReplaceGfxDrawCalls { .. });
                // Append to stage data
                method_queue2.lock().push((epoch, method));
                // If ReplaceGfxDrawCall then wake up miniquad
                if is_replace_dc {
                    miniquad::window::schedule_update();
                }
            }
        });
        god.fg_runtime.push_task(sink_task);

        let white_texture = ctx.new_texture_from_rgba8(1, 1, &[255, 255, 255, 255]);

        let mut shader_meta: ShaderMeta = shader::meta();
        shader_meta.uniforms.uniforms.push(UniformDesc::new("Projection", UniformType::Mat4));
        shader_meta.uniforms.uniforms.push(UniformDesc::new("Model", UniformType::Mat4));

        let shader = ctx
            .new_shader(
                match ctx.info().backend {
                    Backend::OpenGl => ShaderSource::Glsl {
                        vertex: shader::GL_VERTEX,
                        fragment: shader::GL_FRAGMENT,
                    },
                    Backend::Metal => ShaderSource::Msl { program: shader::METAL },
                },
                shader_meta,
            )
            .unwrap();

        let params = PipelineParams {
            color_blend: Some(BlendState::new(
                Equation::Add,
                BlendFactor::Value(BlendValue::SourceAlpha),
                BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
            )),
            ..Default::default()
        };

        let pipeline = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[
                VertexAttribute::new("in_pos", VertexFormat::Float2),
                VertexAttribute::new("in_color", VertexFormat::Float4),
                VertexAttribute::new("in_uv", VertexFormat::Float2),
            ],
            shader,
            params,
        );

        #[cfg(target_os = "android")]
        let libegl = egl::LibEgl::try_load().expect("Cant load LibEGL");

        Stage {
            ctx,
            #[cfg(target_os = "android")]
            libegl,
            pipeline,
            white_texture,
            draw_calls: HashMap::from([(
                0,
                GfxDrawCall { instrs: vec![], dcs: vec![], z_index: 0, timest: 0 },
            )]),
            batches: HashMap::new(),

            textures: HashMap::new(),
            buffers: HashMap::new(),
            anims: HashMap::new(),

            epoch,
            method_queue,
            event_pub,

            pruner: PruneMethodHeap::new(epoch),
            screen_was_off: false,
            ex,
            #[cfg(target_os = "android")]
            refresh_task: None,
        }
    }

    fn process_method(&mut self, mut method: GraphicsMethod) {
        //debug!(target: "gfx", "Received method: {:?}", method);
        let res = match &mut method {
            GraphicsMethod::NewTexture((width, height, data, gtex_id, _)) => {
                self.method_new_texture(*width, *height, data, *gtex_id)
            }
            GraphicsMethod::DeleteTexture((gtex_id, _)) => self.method_delete_texture(*gtex_id),
            GraphicsMethod::NewVertexBuffer((verts, gbuff_id, _)) => {
                self.method_new_vertex_buffer(verts, *gbuff_id)
            }
            GraphicsMethod::NewIndexBuffer((indices, gbuff_id, _)) => {
                self.method_new_index_buffer(indices, *gbuff_id)
            }
            GraphicsMethod::DeleteBuffer((gbuff_id, _, _)) => self.method_delete_buffer(*gbuff_id),
            GraphicsMethod::NewSeqAnim { id, frames_len, oneshot, tag: _ } => {
                self.method_new_anim(*id, *frames_len, *oneshot)
            }
            GraphicsMethod::UpdateSeqAnim { id, frame_idx, frame, tag: _ } => {
                self.method_update_anim(*id, *frame_idx, frame.clone())
            }
            GraphicsMethod::DeleteSeqAnim((ganim_id, _)) => self.method_delete_anim(*ganim_id),
            GraphicsMethod::ReplaceGfxDrawCalls { batch_id, timest, dcs } => {
                //let debug_strs: Vec<_> = dcs.iter().map(|(_, dc)| dc.debug_str).collect();
                //t!("Commit dc to {batch_id}: {debug_strs:?}");
                let batch = self.batches.get_mut(batch_id).unwrap();
                let dcs = std::mem::take(dcs);
                batch.push(GraphicsMethod::ReplaceGfxDrawCalls {
                    batch_id: *batch_id,
                    timest: *timest,
                    dcs,
                });
                if DEBUG_TRAX {
                    get_trax().lock().put_stat(0);
                }
                Ok(())
            }
            GraphicsMethod::StartBatch((batch_id, _debug_str)) => {
                //t!("Start batch {batch_id}: {debug_str:?}");
                if !self.batches.insert(*batch_id, vec![]).is_none() {
                    panic!("Batch {batch_id} already open!")
                }
                if DEBUG_TRAX {
                    get_trax().lock().put_stat(0);
                }
                Ok(())
            }
            GraphicsMethod::EndBatch(batch_id) => {
                //t!("End batch {batch_id}");
                let batch = self.batches.remove(batch_id).unwrap();
                for mut method in batch {
                    let res = match &mut method {
                        GraphicsMethod::ReplaceGfxDrawCalls { batch_id: _, timest, dcs } => {
                            let dcs = std::mem::take(dcs);
                            self.method_replace_draw_calls(*timest, dcs)
                        }
                        _ => panic!("unexpected method in batch!"),
                    };
                    if let Err(err) = res {
                        e!("process_method(method={method:?}) failed with err: {err:?}");
                        panic!("process_method failed!")
                    }
                }
                Ok(())
            }
        };
        if let Err(err) = res {
            e!("process_method(method={method:?}) failed with err: {err:?}");
            panic!("process_method failed!")
        }
    }

    fn method_new_texture(
        &mut self,
        width: u16,
        height: u16,
        data: &Vec<u8>,
        gfx_texture_id: TextureId,
    ) -> Result<()> {
        let texture = self.ctx.new_texture_from_rgba8(width, height, data);
        if DEBUG_GFXAPI {
            debug!(target: "gfx", "Invoked method: new_texture({}, {}, ..., {}) -> {:?}",
                   width, height, gfx_texture_id, texture);
            //debug!(target: "gfx", "Invoked method: new_texture({}, {}, ..., {}) -> {:?}\n{}",
            //       width, height, gfx_texture_id, texture,
            //       ansi_texture(width as usize, height as usize, &data));
        }
        if let Some(_) = self.textures.insert(gfx_texture_id, texture) {
            if DEBUG_TRAX {
                get_trax().lock().put_stat(2);
            }
            //panic!("Duplicate texture ID={gfx_texture_id} detected!");
            return Err(Error::GfxDuplicateTextureID)
        }
        if DEBUG_TRAX {
            get_trax().lock().put_stat(0);
        }
        Ok(())
    }
    fn method_delete_texture(&mut self, gfx_texture_id: TextureId) -> Result<()> {
        let Some(texture) = self.textures.remove(&gfx_texture_id) else {
            if DEBUG_TRAX {
                get_trax().lock().put_stat(2);
            }
            //.expect("couldn't find gfx_texture_id");
            return Err(Error::GfxUnknownTextureID)
        };
        if DEBUG_GFXAPI {
            debug!(target: "gfx", "Invoked method: delete_texture({} => {:?})",
                   gfx_texture_id, texture);
        }
        self.ctx.delete_texture(texture);
        if DEBUG_TRAX {
            get_trax().lock().put_stat(0);
        }
        Ok(())
    }
    fn method_new_vertex_buffer(
        &mut self,
        verts: &[Vertex],
        gfx_buffer_id: BufferId,
    ) -> Result<()> {
        let buffer = self.ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(verts),
        );
        if DEBUG_GFXAPI {
            debug!(target: "gfx", "Invoked method: new_vertex_buffer(..., {}) -> {:?}",
                   gfx_buffer_id, buffer);
            //debug!(target: "gfx", "Invoked method: new_vertex_buffer({:?}, {}) -> {:?}",
            //       verts, gfx_buffer_id, buffer);
        }
        if let Some(_) = self.buffers.insert(gfx_buffer_id, buffer) {
            if DEBUG_TRAX {
                get_trax().lock().put_stat(2);
            }
            //panic!("Duplicate vertex buffer ID={gfx_buffer_id} detected!");
            return Err(Error::GfxDuplicateBufferID)
        }
        if DEBUG_TRAX {
            get_trax().lock().put_stat(0);
        }
        Ok(())
    }
    fn method_new_index_buffer(&mut self, indices: &[u16], gfx_buffer_id: BufferId) -> Result<()> {
        let buffer = self.ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&indices),
        );
        if DEBUG_GFXAPI {
            debug!(target: "gfx", "Invoked method: new_index_buffer({}) -> {:?}",
                   gfx_buffer_id, buffer);
            //debug!(target: "gfx", "Invoked method: new_index_buffer({:?}, {}) -> {:?}",
            //       indices, gfx_buffer_id, buffer);
        }
        if let Some(_) = self.buffers.insert(gfx_buffer_id, buffer) {
            if DEBUG_TRAX {
                get_trax().lock().put_stat(2);
            }
            //panic!("Duplicate index buffer ID={gfx_buffer_id} detected!");
            return Err(Error::GfxDuplicateBufferID)
        }
        if DEBUG_TRAX {
            get_trax().lock().put_stat(0);
        }
        Ok(())
    }
    fn method_delete_buffer(&mut self, gfx_buffer_id: BufferId) -> Result<()> {
        let Some(buffer) = self.buffers.remove(&gfx_buffer_id) else {
            if DEBUG_TRAX {
                get_trax().lock().put_stat(2);
            }
            //.expect("couldn't find gfx_buffer_id");
            return Err(Error::GfxUnknownBufferID)
        };
        if DEBUG_GFXAPI {
            debug!(target: "gfx", "Invoked method: delete_buffer({} => {:?})",
                   gfx_buffer_id, buffer);
        }
        self.ctx.delete_buffer(buffer);
        if DEBUG_TRAX {
            get_trax().lock().put_stat(0);
        }
        Ok(())
    }
    fn method_new_anim(
        &mut self,
        gfx_anim_id: AnimId,
        frames_len: usize,
        oneshot: bool,
    ) -> Result<()> {
        if DEBUG_GFXAPI {
            debug!(target: "gfx", "Invoked method: new_anim({gfx_anim_id}, {frames_len}, {oneshot})");
        }
        if let Some(_) = self.anims.insert(gfx_anim_id, GfxSeqAnim::new(frames_len, oneshot)) {
            //panic!("Duplicate index buffer ID={gfx_buffer_id} detected!");
            return Err(Error::GfxDuplicateAnimID)
        }
        Ok(())
    }
    fn method_update_anim(
        &mut self,
        gfx_anim_id: AnimId,
        frame_idx: usize,
        frame: AnimFrame,
    ) -> Result<()> {
        let Some(anim) = self.anims.get_mut(&gfx_anim_id) else {
            //.expect("couldn't find gfx_anim_id");
            return Err(Error::GfxUnknownAnimID)
        };
        if DEBUG_GFXAPI {
            debug!(target: "gfx", "Invoked method: update_anim({gfx_anim_id}[{frame_idx}] => {frame:?})");
        }
        anim.set(frame_idx, frame, &self.textures, &self.buffers);
        Ok(())
    }
    fn method_delete_anim(&mut self, gfx_anim_id: AnimId) -> Result<()> {
        let Some(anim) = self.anims.remove(&gfx_anim_id) else {
            //.expect("couldn't find gfx_anim_id");
            return Err(Error::GfxUnknownAnimID)
        };
        if DEBUG_GFXAPI {
            debug!(target: "gfx", "Invoked method: delete_anim({} => {:?})", gfx_anim_id, anim);
        }
        Ok(())
    }
    fn method_replace_draw_calls(
        &mut self,
        timest: Timestamp,
        dcs: Vec<(DcId, DrawCall)>,
    ) -> Result<()> {
        if DEBUG_GFXAPI {
            debug!(target: "gfx", "Invoked method: replace_draw_calls({:?})", dcs);
        }
        for (key, val) in dcs {
            let Some(val) = val.compile(&self.textures, &self.buffers, timest) else {
                if DEBUG_TRAX {
                    get_trax().lock().put_stat(3);
                }
                error!(target: "gfx", "fatal: replace_draw_calls({timest}, ...) failed with item ID={key}");
                continue
            };
            //self.draw_calls.insert(key, val);
            match self.draw_calls.get_mut(&key) {
                Some(old_val) => {
                    // Only replace the draw call if it is more recent
                    if old_val.timest < timest {
                        if DEBUG_TRAX {
                            get_trax().lock().put_stat(0);
                        }
                        *old_val = val;
                    } else {
                        trace!(target: "gfx", "Rejected stale draw_call {key}: {val:?}");
                        if DEBUG_TRAX {
                            get_trax().lock().put_stat(2);
                        }
                    }
                }
                None => {
                    self.draw_calls.insert(key, val);
                    if DEBUG_TRAX {
                        get_trax().lock().put_stat(1);
                    }
                }
            }
        }
        Ok(())
    }

    fn trax_method(&self, epoch: EpochIndex, method: &GraphicsMethod) {
        let mut trax = get_trax().lock();
        match method {
            GraphicsMethod::NewTexture((_, _, _, gtex_id, tag)) => {
                trax.put_tex(epoch, *gtex_id, *tag);
            }
            GraphicsMethod::DeleteTexture((gtex_id, tag)) => {
                trax.del_tex(epoch, *gtex_id, *tag);
            }
            GraphicsMethod::NewVertexBuffer((verts, gbuff_id, tag)) => {
                trax.put_verts(epoch, verts.clone(), *gbuff_id, *tag, 0);
            }
            GraphicsMethod::NewIndexBuffer((idxs, gbuff_id, tag)) => {
                trax.put_idxs(epoch, idxs.clone(), *gbuff_id, *tag, 1);
            }
            GraphicsMethod::DeleteBuffer((gbuff_id, tag, buftype)) => {
                trax.del_buf(epoch, *gbuff_id, *tag, *buftype);
            }
            GraphicsMethod::NewSeqAnim { id, frames_len, oneshot, tag } => {
                //trax.put_idxs(epoch, idxs.clone(), *gbuff_id, *tag, 1);
            }
            GraphicsMethod::UpdateSeqAnim { .. } => {
                //trax.put_idxs(epoch, idxs.clone(), *gbuff_id, *tag, 1);
            }
            GraphicsMethod::DeleteSeqAnim((ganim_id, tag)) => {
                //trax.del_buf(epoch, *gbuff_id, *tag, *buftype);
            }
            GraphicsMethod::ReplaceGfxDrawCalls { batch_id, timest, dcs } => {
                trax.put_dcs(epoch, *batch_id, *timest, dcs);
            }
            GraphicsMethod::StartBatch((batch_id, debug_str)) => {
                trax.put_start_batch(epoch, *batch_id, *debug_str);
            }
            GraphicsMethod::EndBatch(batch_id) => {
                trax.put_end_batch(epoch, *batch_id);
            }
        };
    }

    fn egl_ctx_is_disabled(&self) -> bool {
        #[cfg(target_os = "android")]
        {
            let egl_ctx = unsafe { (self.libegl.eglGetCurrentContext)() };
            egl_ctx.is_null()
        }
        #[cfg(not(target_os = "android"))]
        false
    }
}

/// This is used to process the method queue while the screen is off to avoid the queue
/// becoming congested and using up all the memory.
/// Will drop alloc/delete pairs, and merge draw calls together.
struct PruneMethodHeap {
    /// Newly allocated buffers while screen was off
    new_buf: HashMap<BufferId, GraphicsMethod>,
    /// Newly allocated textures while screen was off
    new_tex: HashMap<TextureId, GraphicsMethod>,
    /// Deleted objects
    del: Vec<GraphicsMethod>,
    /// Draw calls
    dcs: HashMap<DcId, (BatchGuardId, Timestamp, DrawCall)>,

    epoch: EpochIndex,
}

impl PruneMethodHeap {
    fn new(epoch: EpochIndex) -> Self {
        Self {
            new_buf: HashMap::new(),
            new_tex: HashMap::new(),
            del: vec![],
            dcs: HashMap::new(),
            epoch,
        }
    }

    fn drain(&mut self, methods: Vec<(EpochIndex, GraphicsMethod)>) {
        // Process as many methods as we can
        for (epoch, method) in methods {
            if epoch < self.epoch {
                // Discard old rubbish
                trace!(target: "gfx::pruner", "Discard method with old epoch: {epoch} curr: {} [method={method:?}]", self.epoch);
                continue
            }
            assert_eq!(epoch, self.epoch);
            self.process_method(method);
        }
    }

    fn process_method(&mut self, method: GraphicsMethod) {
        match method.clone() {
            GraphicsMethod::NewTexture((_, _, _, gtex_id, _)) => {
                self.new_tex.insert(gtex_id, method);
            }
            GraphicsMethod::DeleteTexture((gtex_id, _)) => {
                if self.new_tex.remove(&gtex_id).is_none() {
                    self.del.push(method);
                }
            }
            GraphicsMethod::NewVertexBuffer((_, gbuff_id, _)) => {
                self.new_buf.insert(gbuff_id, method);
            }
            GraphicsMethod::NewIndexBuffer((_, gbuff_id, _)) => {
                self.new_buf.insert(gbuff_id, method);
            }
            GraphicsMethod::DeleteBuffer((gbuff_id, _, _)) => {
                if self.new_buf.remove(&gbuff_id).is_none() {
                    self.del.push(method);
                }
            }
            GraphicsMethod::NewSeqAnim { id: _, frames_len, oneshot, tag } => {
                //self.new_buf.insert(gbuff_id, method);
            }
            GraphicsMethod::UpdateSeqAnim { .. } => {
                //self.new_buf.insert(gbuff_id, method);
            }
            GraphicsMethod::DeleteSeqAnim((ganim_id, _)) => {
                //if self.new_buf.remove(&gbuff_id).is_none() {
                //    self.del.push(method);
                //}
            }
            GraphicsMethod::ReplaceGfxDrawCalls { batch_id, timest, dcs } => {
                self.method_replace_draw_calls(batch_id, timest, dcs)
            }
            // Discard batches since we will apply everything all at once anyway
            // once the screen is switched on.
            GraphicsMethod::StartBatch(_) => {}
            GraphicsMethod::EndBatch(_) => {}
        }
    }

    fn method_replace_draw_calls(
        &mut self,
        batch_id: BatchGuardId,
        timest: Timestamp,
        dcs: Vec<(DcId, DrawCall)>,
    ) {
        for (key, val) in dcs {
            match self.dcs.get_mut(&key) {
                Some(old_val) => {
                    // Only replace the draw call if it is more recent
                    if old_val.1 < timest {
                        *old_val = (batch_id, timest, val);
                    } else {
                        trace!(target: "gfx::pruner", "Rejected stale draw_call {key}: {val:?}");
                    }
                }
                None => {
                    self.dcs.insert(key, (batch_id, timest, val));
                }
            }
        }
    }

    /// Collect everything now the screen is on
    fn recv_all(&mut self) -> Vec<GraphicsMethod> {
        // Inhale that smoke deep
        let mut meth = Vec::with_capacity(
            self.new_buf.len() + self.new_tex.len() + self.del.len() + self.dcs.len(),
        );
        let new_buf = std::mem::take(&mut self.new_buf);
        let new_tex = std::mem::take(&mut self.new_tex);
        meth.extend(new_buf.into_values());
        meth.extend(new_tex.into_values());
        meth.append(&mut self.del);
        for (dc_id, (batch_id, timest, dc)) in std::mem::take(&mut self.dcs) {
            meth.push(GraphicsMethod::ReplaceGfxDrawCalls {
                batch_id,
                timest,
                dcs: vec![(dc_id, dc)],
            });
        }
        meth
    }
}

impl EventHandler for Stage {
    fn update(&mut self) {
        // todo: trax is all messed up in this func

        let methods = std::mem::take(&mut *self.method_queue.lock());

        if self.egl_ctx_is_disabled() {
            #[cfg(target_os = "android")]
            {
                self.refresh_task = None;
            }

            // Immediately apply any pending batches when the screen is switched off
            let batch_ids: Vec<_> = self.batches.keys().cloned().collect();
            for batch_id in batch_ids {
                self.process_method(GraphicsMethod::EndBatch(batch_id));
            }
            self.batches.clear();

            // Screen is off so collect all methods into the pruner
            self.pruner.drain(methods);
            self.screen_was_off = true;
            return
        }

        #[cfg(target_os = "android")]
        if self.refresh_task.is_none() {
            // For animations do periodic refresh every 40 ms
            self.refresh_task = Some(self.ex.spawn(async move {
                loop {
                    darkfi::system::msleep(40).await;
                    miniquad::window::schedule_update();
                }
            }));
        }

        // We actually want to skip draining the prune queue the first time so
        // draw actually gets a chance to be called first.
        // Otherwise we will just see a black screen for a sec or so.
        if self.screen_was_off {
            self.screen_was_off = false;
        } else {
            let methods = self.pruner.recv_all();
            assert!(methods.is_empty() || self.batches.is_empty());
            // Process all cached methods by the pruner from while the screen was off.
            for method in methods {
                // Stale methods will be dropped by pruner, so they will not be caught by trax
                // while the screen is off.
                if DEBUG_TRAX {
                    self.trax_method(self.epoch, &method);
                }
                // We discard batches here but process_method uses them so implement this
                // workaround.
                match method {
                    GraphicsMethod::ReplaceGfxDrawCalls { batch_id: _, timest, dcs } => {
                        if let Err(err) = self.method_replace_draw_calls(timest, dcs) {
                            e!("process_method for ReplaceGfxDrawCalls failed err: {err:?}");
                            panic!("process_method failed!")
                        }
                    }
                    _ => self.process_method(method),
                }
                if DEBUG_TRAX {
                    get_trax().lock().flush();
                }
            }
        }

        // Process as many methods as we can
        for (epoch, method) in methods {
            if DEBUG_TRAX {
                self.trax_method(epoch, &method);
            }
            if epoch < self.epoch {
                if DEBUG_TRAX {
                    let mut trax = get_trax().lock();
                    trax.put_stat(1);
                    trax.flush();
                }
                // Discard old rubbish
                trace!(target: "gfx", "Discard method with old epoch: {epoch} curr: {} [method={method:?}]", self.epoch);
                continue
            }
            assert_eq!(epoch, self.epoch);
            self.process_method(method);
            if DEBUG_TRAX {
                get_trax().lock().flush();
            }
        }
    }

    fn draw(&mut self) {
        self.ctx.begin_default_pass(PassAction::clear_color(0., 0., 0., 1.));
        self.ctx.apply_pipeline(&self.pipeline);

        // This will make the top left (0, 0) and the bottom right (1, 1)
        // Default is (-1, 1) -> (1, -1)
        let proj = glam::Mat4::from_translation(glam::Vec3::new(-1., 1., 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(2., -2., 1.));

        let mut uniforms_data = [0u8; 128];
        let data: [u8; 64] = unsafe { std::mem::transmute_copy(&proj) };
        uniforms_data[0..64].copy_from_slice(&data);
        //let data: [u8; 64] = unsafe { std::mem::transmute_copy(&model) };
        //uniforms_data[64..].copy_from_slice(&data);
        assert_eq!(128, 2 * UniformType::Mat4.size());

        let (screen_w, screen_h) = miniquad::window::screen_size();

        // Mark all anims as invisible
        for (_, anim) in self.anims.iter_mut() {
            anim.is_visible = false;
        }

        let mut render_ctx = RenderContext {
            ctx: &mut self.ctx,
            draw_calls: &self.draw_calls,
            uniforms_data,
            white_texture: self.white_texture,
            scale: 1.,
            view: Rectangle::from([0., 0., screen_w, screen_h]),
            cursor: Point::from([0., 0.]),
            anims: &mut self.anims,
        };
        render_ctx.draw();

        self.ctx.commit_frame();
    }

    fn resize_event(&mut self, width: f32, height: f32) {
        t!("resize_event({width}, {height})");
        let filename = get_window_size_filename();
        if let Some(parent) = filename.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = File::create(filename) {
            (width as i32).encode(&mut file).unwrap();
            (height as i32).encode(&mut file).unwrap();
        }

        self.event_pub.notify_resize(Dimension::from([width, height]));
    }

    fn key_down_event(&mut self, keycode: KeyCode, mods: KeyMods, repeat: bool) {
        self.event_pub.notify_key_down(keycode, mods, repeat);
    }
    fn key_up_event(&mut self, keycode: KeyCode, mods: KeyMods) {
        self.event_pub.notify_key_up(keycode, mods);
    }
    fn char_event(&mut self, chr: char, mods: KeyMods, repeat: bool) {
        self.event_pub.notify_char(chr, mods, repeat);
    }

    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        let pos = Point::from([x, y]);
        self.event_pub.notify_mouse_btn_down(button, pos);
    }
    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        let pos = Point::from([x, y]);
        self.event_pub.notify_mouse_btn_up(button, pos);
    }
    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        let pos = Point::from([x, y]);
        self.event_pub.notify_mouse_move(pos);
    }
    fn mouse_wheel_event(&mut self, x: f32, y: f32) {
        let pos = Point::from([x, y]);
        self.event_pub.notify_mouse_wheel(pos);
    }

    /// The id corresponds to multi-touch. Multiple touch events have different ids.
    fn touch_event(&mut self, phase: TouchPhase, id: u64, x: f32, y: f32) {
        let pos = Point::from([x, y]);
        self.event_pub.notify_touch(phase, id, pos);
    }

    fn quit_requested_event(&mut self) {
        debug!(target: "gfx", "quit requested");
        let god = GOD.get().unwrap();
        god.stop_app();
    }
}

pub fn run_gui(linux_backend: miniquad::conf::LinuxBackend) {
    let mut window_width = 1024;
    let mut window_height = 768;
    if let Ok(mut file) = File::open(get_window_size_filename()) {
        window_width = Decodable::decode(&mut file).unwrap();
        window_height = Decodable::decode(&mut file).unwrap();
    }
    debug!(target: "gfx", "Window size {window_width} x {window_height}");

    let mut conf = miniquad::conf::Conf {
        window_title: "DarkFi".to_string(),
        window_width,
        window_height,
        high_dpi: true,
        window_resizable: true,
        platform: miniquad::conf::Platform {
            linux_backend,
            #[cfg(target_os = "android")]
            blocking_event_loop: true,
            android_panic_hook: false,
            ..Default::default()
        },
        icon: Some(miniquad::conf::Icon {
            small: favico::SMALL,
            medium: favico::MEDIUM,
            big: favico::BIG,
        }),
        ..Default::default()
    };
    let metal = std::env::args().nth(1).as_deref() == Some("metal");
    conf.platform.apple_gfx_api =
        if metal { conf::AppleGfxApi::Metal } else { conf::AppleGfxApi::OpenGl };

    miniquad::start(conf, || Box::new(Stage::new()));
}
