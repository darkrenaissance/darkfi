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

use darkfi_serial::{
    async_trait, AsyncEncodable, AsyncWrite, Decodable, Encodable, FutAsyncWriteExt,
    SerialDecodable, SerialEncodable, VarInt,
};
#[cfg(target_os = "android")]
use miniquad::native::egl;
use miniquad::{
    conf, window, Bindings, BufferSource, BufferType, BufferUsage, EventHandler, KeyCode, KeyMods,
    MouseButton, PassAction, Pipeline, RenderingBackend, TextureFormat, TextureKind, TextureParams,
    TextureWrap, TouchPhase, UniformType,
};
use std::{
    collections::{HashMap, HashSet},
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
mod api;
pub use api::{
    EpochIndex, GraphicsMethod, ManagedBuffer, ManagedBufferPtr, ManagedSeqAnim, ManagedSeqAnimPtr,
    ManagedTexture, ManagedTexturePtr, RenderApi, RenderApiSync,
};
mod ev;
pub use ev::{
    GraphicsEventCharSub, GraphicsEventKeyDownSub, GraphicsEventKeyUpSub,
    GraphicsEventMouseButtonDownSub, GraphicsEventMouseButtonUpSub, GraphicsEventMouseMoveSub,
    GraphicsEventMouseWheelSub, GraphicsEventPublisher, GraphicsEventPublisherPtr,
    GraphicsEventResizeSub, GraphicsEventTouchSub,
};
mod favico;
mod prune;
use prune::PruneMethodHeap;
mod linalg;
pub use linalg::{Dimension, Point, Rectangle};
mod shader;
mod trax;
use trax::get_trax;

use crate::{
    prop::{BatchGuardId, PropertyAtomicGuard},
    scene::{Pimpl, SceneNodePtr},
    util::unixtime,
    GOD,
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

macro_rules! d { ($($arg:tt)*) => { debug!(target: "gfx", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "gfx", $($arg)*); } }

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

#[derive(Clone, Debug)]
pub struct DrawMesh {
    pub vertex_buffer: ManagedBufferPtr,
    pub index_buffer: ManagedBufferPtr,
    pub textures: Option<Vec<ManagedTexturePtr>>,
    pub num_elements: i32,
}

impl DrawMesh {
    fn compile(
        self,
        textures: &HashMap<TextureId, miniquad::TextureId>,
        buffers: &HashMap<BufferId, miniquad::BufferId>,
        debug_str: &'static str,
    ) -> GfxDrawMesh {
        let vertex_buffer_id = self.vertex_buffer.id;
        let index_buffer_id = self.index_buffer.id;
        let _buffers_keep_alive = [self.vertex_buffer, self.index_buffer];

        let textures = match self.textures {
            Some(gfx_textures) => {
                let mut compiled = Vec::with_capacity(gfx_textures.len());
                for gfx_texture in gfx_textures {
                    compiled.push(Self::get_texture(textures, gfx_texture, debug_str));
                }
                Some(compiled)
            }
            None => None,
        };

        GfxDrawMesh {
            vertex_buffer: Self::get_buffer(buffers, vertex_buffer_id, debug_str),
            index_buffer: Self::get_buffer(buffers, index_buffer_id, debug_str),
            _buffers_keep_alive,
            textures,
            num_elements: self.num_elements,
        }
    }

    fn get_texture(
        textures: &HashMap<TextureId, miniquad::TextureId>,
        gfx_texture: ManagedTexturePtr,
        debug_str: &'static str,
    ) -> (ManagedTexturePtr, miniquad::TextureId) {
        let gfx_texture_id = gfx_texture.id;

        let Some(_mq_texture_id) = textures.get(&gfx_texture_id) else {
            panic!("Missing texture ID={gfx_texture_id} debug={debug_str}")
        };

        (gfx_texture, textures[&gfx_texture_id])
    }

    fn get_buffer(
        buffers: &HashMap<BufferId, miniquad::BufferId>,
        gfx_buffer_id: BufferId,
        debug_str: &'static str,
    ) -> miniquad::BufferId {
        let Some(mq_buffer_id) = buffers.get(&gfx_buffer_id) else {
            panic!("Missing buffer ID={gfx_buffer_id} debug={debug_str}")
        };
        *mq_buffer_id
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
        match &self.textures {
            Some(texs) => {
                len += 1u8.encode(s)?;
                len += VarInt(texs.len() as u64).encode(s)?;
                for t in texs {
                    len += t.id.encode(s)?;
                    len += t.epoch.encode(s)?;
                    len += t.tag.encode(s)?;
                }
            }
            None => {
                len += 0u8.encode(s)?;
            }
        }
        len += self.num_elements.encode(s)?;
        Ok(len)
    }
}

#[derive(Clone, Copy, Debug, SerialEncodable)]
pub enum GraphicPipeline {
    RGB,
    YUV,
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

#[derive(Debug, Clone)]
pub enum DrawInstruction {
    SetScale(f32),
    Move(Point),
    SetPos(Point),
    ApplyView(Rectangle),
    Draw(DrawMesh),
    Animation(ManagedSeqAnimPtr),
    EnableDebug,
    SetPipeline(GraphicPipeline),
    Overlay(Vec<DrawInstruction>),
}

impl DrawInstruction {
    fn compile(
        self,
        textures: &HashMap<TextureId, miniquad::TextureId>,
        buffers: &HashMap<BufferId, miniquad::BufferId>,
        debug_str: &'static str,
    ) -> GfxDrawInstruction {
        match self {
            Self::SetScale(scale) => GfxDrawInstruction::SetScale(scale),
            Self::Move(off) => GfxDrawInstruction::Move(off),
            Self::SetPos(pos) => GfxDrawInstruction::SetPos(pos),
            Self::ApplyView(view) => GfxDrawInstruction::ApplyView(view),
            Self::Draw(mesh) => {
                GfxDrawInstruction::Draw(mesh.compile(textures, buffers, debug_str))
            }
            Self::Animation(anim) => GfxDrawInstruction::Animation(anim),
            Self::EnableDebug => GfxDrawInstruction::EnableDebug,
            Self::SetPipeline(pipeline) => GfxDrawInstruction::SetPipeline(pipeline),
            Self::Overlay(instrs) => {
                let compiled =
                    instrs.into_iter().map(|i| i.compile(textures, buffers, debug_str)).collect();
                GfxDrawInstruction::Overlay(compiled)
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
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
    ) -> GfxDrawCall {
        GfxDrawCall {
            instrs: self
                .instrs
                .into_iter()
                .map(|i| i.compile(textures, buffers, self.debug_str))
                .collect(),
            dcs: self.dcs,
            z_index: self.z_index,
            timest,
        }
    }
}

#[derive(Clone, Debug)]
struct GfxDrawMesh {
    vertex_buffer: miniquad::BufferId,
    index_buffer: miniquad::BufferId,
    /// Keeps the buffers alive for the duration of this draw call
    _buffers_keep_alive: [ManagedBufferPtr; 2],
    textures: Option<Vec<(ManagedTexturePtr, miniquad::TextureId)>>,
    num_elements: i32,
}

#[derive(Debug, Clone)]
enum GfxDrawInstruction {
    SetScale(f32),
    Move(Point),
    SetPos(Point),
    ApplyView(Rectangle),
    Draw(GfxDrawMesh),
    Animation(ManagedSeqAnimPtr),
    EnableDebug,
    SetPipeline(GraphicPipeline),
    Overlay(Vec<GfxDrawInstruction>),
}

#[derive(Clone, Debug)]
struct GfxDrawCall {
    instrs: Vec<GfxDrawInstruction>,
    dcs: Vec<DcId>,
    z_index: u32,
    timest: Timestamp,
}

struct OverlayDefer {
    scale: f32,
    pos: Point,
    instrs: Vec<GfxDrawInstruction>,
}

struct RenderContext<'a> {
    ctx: &'a mut Box<dyn RenderingBackend>,
    draw_calls: &'a HashMap<DcId, GfxDrawCall>,
    uniforms_data: [u8; 128],
    white_texture: miniquad::TextureId,
    loaded_pipelines: &'a [Pipeline; 2],

    scale: f32,
    view: Rectangle,
    cursor: Point,
    gfx_pipeline: GraphicPipeline,

    anims: &'a mut HashMap<AnimId, GfxSeqAnim>,
    overlays: Vec<OverlayDefer>,
}

impl<'a> RenderContext<'a> {
    fn draw(&mut self) {
        if DEBUG_RENDER {
            let screen_size = miniquad::window::screen_size();
            d!("RenderContext::draw() [screen_size={screen_size:?}]");
        }
        if DEBUG_TRAX {
            get_trax().lock().set_curr(0);
        }
        self.draw_call(&self.draw_calls[&0], 0, DEBUG_RENDER);
        if DEBUG_RENDER {
            d!("RenderContext::draw() [DONE]");
        }
        // View should be reset now so draw overlays
        self.draw_overlays();
    }

    fn draw_overlays(&mut self) {
        let (screen_w, screen_h) = miniquad::window::screen_size();

        let overlays = std::mem::take(&mut self.overlays);
        for overlay in overlays {
            self.view = Rectangle::new(0., 0., screen_w, screen_h);
            self.scale = overlay.scale;
            self.view.w /= self.scale;
            self.view.h /= self.scale;
            self.apply_view();

            self.cursor = overlay.pos;
            self.apply_model();

            for instr in overlay.instrs {
                match instr {
                    GfxDrawInstruction::Move(off) => {
                        self.cursor += off;
                        self.apply_model();
                    }
                    GfxDrawInstruction::Draw(mesh) => {
                        if DEBUG_RENDER {
                            d!("    draw_overlay({mesh:?})");
                        }
                        let images = match &mesh.textures {
                            Some(texs) => texs.iter().map(|(_, tex_id)| *tex_id).collect(),
                            None => vec![self.white_texture],
                        };
                        let bindings = Bindings {
                            vertex_buffers: vec![mesh.vertex_buffer],
                            index_buffer: mesh.index_buffer,
                            images,
                        };
                        self.ctx.apply_bindings(&bindings);
                        self.ctx.draw(0, mesh.num_elements, 1);
                    }
                    _ => unimplemented!(),
                }
            }
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
            d!("=> viewport {view_x} {view_y} {view_w} {view_h}");
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
        let old_pipeline = self.gfx_pipeline;

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
                        d!("{ws}set_scale({scale})");
                    }
                }
                GfxDrawInstruction::Move(off) => {
                    self.cursor += *off;
                    if is_debug {
                        d!(
                            "{ws}move({off:?})  cursor={:?}, scale={}, view={:?}",
                            self.cursor,
                            self.scale,
                            self.view
                        );
                    }
                    self.apply_model();
                }
                GfxDrawInstruction::SetPos(pos) => {
                    self.cursor = old_cursor + *pos;
                    if is_debug {
                        d!(
                            "{ws}set_pos({pos:?})  cursor={:?}, scale={}, view={:?}",
                            self.cursor,
                            self.scale,
                            self.view
                        );
                    }
                    self.apply_model();
                }
                GfxDrawInstruction::ApplyView(view) => {
                    // Adjust view relative to cursor
                    self.view = *view + self.cursor + old_view.pos();

                    // We could just skip drawing when clipping rect isn't visible
                    // using an is_visible flag.
                    match self.view.clip(&old_view) {
                        Some(clipped) => self.view = clipped,
                        None => self.view = Rectangle::zero(),
                    }

                    // Cursor resets within the view
                    self.cursor = Point::zero();
                    if is_debug {
                        d!("{ws}apply_view({view:?})  scale={}, view={:?}", self.scale, self.view);
                    }
                    self.apply_view();
                    self.apply_model();
                }
                GfxDrawInstruction::Draw(mesh) => {
                    if is_debug {
                        d!("{ws}draw({mesh:?})");
                    }
                    let images = match &mesh.textures {
                        Some(texs) => texs.iter().map(|(_, tex_id)| *tex_id).collect(),
                        None => vec![self.white_texture],
                    };
                    let bindings = Bindings {
                        vertex_buffers: vec![mesh.vertex_buffer],
                        index_buffer: mesh.index_buffer,
                        images,
                    };
                    self.ctx.apply_bindings(&bindings);
                    self.ctx.draw(0, mesh.num_elements, 1);
                }
                GfxDrawInstruction::Animation(anim) => {
                    let gfx_anim = self.anims.get_mut(&anim.id).unwrap();
                    gfx_anim.is_visible = true;
                    if let Some(dc) = gfx_anim.tick() {
                        self.draw_call(&dc, indent + 1, is_debug);
                    }
                }
                GfxDrawInstruction::EnableDebug => {
                    if !is_debug {
                        indent = 0;
                    }
                    is_debug = true;
                    d!("Frame start");
                }
                GfxDrawInstruction::SetPipeline(pipeline) => {
                    self.gfx_pipeline = *pipeline;
                    let pipeline_idx = *pipeline as usize;
                    assert!(pipeline_idx < self.loaded_pipelines.len());
                    self.ctx.apply_pipeline(&self.loaded_pipelines[pipeline_idx]);
                    if is_debug {
                        d!("{ws}set_pipeline({pipeline:?})");
                    }
                }
                GfxDrawInstruction::Overlay(instrs) => {
                    let pos = self.view.pos() / self.scale + self.cursor;
                    self.overlays.push(OverlayDefer {
                        scale: self.scale,
                        pos,
                        instrs: instrs.clone(),
                    });
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
                d!("{ws}drawcall {dc_key}");
            }
            self.draw_call(dc, indent + 1, is_debug);
        }

        self.scale = old_scale;

        if is_debug {
            d!("{ws}Frame close: cursor={old_cursor:?}, view={old_view:?}");
        }

        self.view = old_view;
        self.apply_view();

        self.cursor = old_cursor;
        self.gfx_pipeline = old_pipeline;
        let pipeline_idx = self.gfx_pipeline as usize;
        assert!(pipeline_idx < self.loaded_pipelines.len());
        self.ctx.apply_pipeline(&self.loaded_pipelines[pipeline_idx]);
        self.apply_model();
    }
}

type Timestamp = u64;
type DcId = u64;

struct Stage {
    ctx: Box<dyn RenderingBackend>,
    #[cfg(target_os = "android")]
    libegl: egl::LibEgl,
    loaded_pipelines: [Pipeline; 2],
    white_texture: miniquad::TextureId,
    draw_calls: HashMap<DcId, GfxDrawCall>,
    pending_batches: HashMap<BatchGuardId, Vec<GraphicsMethod>>,
    /// When dropping batches, we add to this set so that we keep track
    /// of the internal state's correctness.
    dropped_batches: Box<HashSet<BatchGuardId>>,

    textures: Box<HashMap<TextureId, miniquad::TextureId>>,
    buffers: Box<HashMap<BufferId, miniquad::BufferId>>,
    anims: Box<HashMap<AnimId, GfxSeqAnim>>,

    epoch: EpochIndex,
    method_recv: async_channel::Receiver<(EpochIndex, GraphicsMethod)>,
    event_pub: GraphicsEventPublisherPtr,
    render_api: RenderApi,

    pruner: PruneMethodHeap,
    screen_state: ScreenState,

    /// Cached window node, looked up lazily on first touch event
    window_node: Option<SceneNodePtr>,
}

impl Stage {
    pub fn new() -> Self {
        if DEBUG_TRAX {
            get_trax().lock().clear();
        }
        let mut ctx: Box<dyn RenderingBackend> = window::new_rendering_backend();

        let god = GOD.get().unwrap();
        // Start a new epoch. This is a brand new UI run.
        let render_api = god.render_api.clone();
        let epoch = render_api.next_epoch();
        // This will start the app to start. Needed since we cannot get window size for init
        // until window is created.
        god.start_app(epoch);
        let method_recv = god.method_recv.clone();
        let event_pub = god.event_pub.clone();

        let white_texture = ctx.new_texture_from_rgba8(1, 1, &[255, 255, 255, 255]);

        let anims: HashMap<AnimId, GfxSeqAnim> = HashMap::new();

        let rgb_pipeline = shader::create_rgb_pipeline(&mut ctx);
        let yuv_pipeline = shader::create_yuv_pipeline(&mut ctx);

        #[cfg(target_os = "android")]
        let libegl = egl::LibEgl::try_load().expect("Cant load LibEGL");

        let mut self_ = Stage {
            ctx,
            #[cfg(target_os = "android")]
            libegl,
            loaded_pipelines: [rgb_pipeline, yuv_pipeline],
            white_texture,
            draw_calls: HashMap::from([(
                0,
                GfxDrawCall { instrs: vec![], dcs: vec![], z_index: 0, timest: 0 },
            )]),
            pending_batches: HashMap::new(),
            dropped_batches: Box::new(HashSet::new()),

            textures: Box::new(HashMap::new()),
            buffers: Box::new(HashMap::new()),
            anims: Box::new(anims),

            epoch,
            method_recv,
            event_pub,
            render_api,

            pruner: PruneMethodHeap::new(epoch),
            screen_state: ScreenState::On,

            window_node: None,
        };
        self_.pruner.textures = &*self_.textures as *const _;
        self_.pruner.buffers = &*self_.buffers as *const _;
        self_.pruner.anims = &*self_.anims as *const _;
        self_.pruner.dropped_batches = &mut *self_.dropped_batches as *mut _;
        self_
    }

    fn process_method(&mut self, mut method: GraphicsMethod) {
        //d!("Received method: {method:?}");
        match &mut method {
            GraphicsMethod::NewTexture((width, height, data, fmt, gtex_id, _)) => {
                self.method_new_texture(*width, *height, data, *fmt, *gtex_id)
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
            GraphicsMethod::ReplaceGfxDrawCalls { batch_id, ref mut dcs } => {
                match batch_id {
                    Some(bid) => {
                        //let debug_strs: Vec<_> = dcs.iter().map(|(_, dc)| dc.debug_str).collect();
                        //t!("Commit dc to {bid}: {debug_strs:?}");
                        if self.dropped_batches.contains(&bid) {
                            t!("Discarding ReplaceGfxDrawCalls from dropped {bid}");
                            return
                        }
                        let Some(batch) = self.pending_batches.get_mut(&bid) else {
                            panic!("unknown batch {bid}")
                        };
                        let method = std::mem::take(&mut method);
                        batch.push(method);
                    }
                    None => {
                        // Process immediately without batching
                        let timest = unixtime();
                        let dcs = std::mem::take(dcs);
                        self.method_replace_draw_calls(timest, dcs);
                    }
                }
                if DEBUG_TRAX {
                    get_trax().lock().put_stat(0);
                }
            }
            GraphicsMethod::StartBatch { batch_id, tag } => {
                if DEBUG_GFXAPI {
                    t!("Start batch {batch_id}: {tag:?}");
                }
                if !self.pending_batches.insert(*batch_id, vec![]).is_none() {
                    panic!("batch {batch_id} already open!")
                }
                if DEBUG_TRAX {
                    get_trax().lock().put_stat(0);
                }
            }
            GraphicsMethod::EndBatch { batch_id, timest } => {
                if self.dropped_batches.remove(batch_id) {
                    if DEBUG_GFXAPI {
                        t!("End batch {batch_id} was dropped");
                    }
                    return
                }
                if DEBUG_GFXAPI {
                    t!("End batch {batch_id}");
                }
                let Some(batch) = self.pending_batches.remove(batch_id) else {
                    panic!("unknown batch {batch_id}")
                };
                for mut method in batch {
                    match &mut method {
                        GraphicsMethod::ReplaceGfxDrawCalls { batch_id: _, dcs } => {
                            let dcs = std::mem::take(dcs);
                            self.method_replace_draw_calls(*timest, dcs)
                        }
                        _ => panic!("unexpected method in batch!"),
                    }
                }
            }
            GraphicsMethod::Noop => panic!("noop"),
        }
    }

    pub(self) fn method_new_texture(
        &mut self,
        width: u16,
        height: u16,
        data: &Vec<u8>,
        format: TextureFormat,
        gfx_texture_id: TextureId,
    ) {
        let fmt_size = format.size(width as u32, height as u32) as usize;
        assert_eq!(
            fmt_size,
            data.len(),
            "Texture data size mismatch for ID={gfx_texture_id}: \
             expected {fmt_size}, got {} for {width}x{height} {:?}",
            data.len(),
            format
        );
        let texture = self.ctx.new_texture_from_data_and_format(
            data,
            TextureParams {
                kind: TextureKind::Texture2D,
                format,
                width: width as _,
                height: height as _,
                wrap: TextureWrap::Clamp,
                min_filter: miniquad::FilterMode::Linear,
                mag_filter: miniquad::FilterMode::Linear,
                mipmap_filter: miniquad::MipmapFilterMode::None,
                allocate_mipmaps: false,
                sample_count: 1,
            },
        );
        if DEBUG_GFXAPI {
            d!("Invoked method: new_texture({width}, {height}, ..., {gfx_texture_id}) -> {texture:?}");
            //d!("Invoked method: new_texture({}, {}, ..., {}) -> {:?}\n{}",
            //       width, height, gfx_texture_id, texture,
            //       ansi_texture(width as usize, height as usize, &data));
        }
        if let Some(_) = self.textures.insert(gfx_texture_id, texture) {
            if DEBUG_TRAX {
                get_trax().lock().put_stat(2);
            }
            panic!("Duplicate texture ID={gfx_texture_id} detected!");
        }
        if DEBUG_TRAX {
            get_trax().lock().put_stat(0);
        }
    }
    pub(self) fn method_delete_texture(&mut self, gfx_texture_id: TextureId) {
        let Some(texture) = self.textures.remove(&gfx_texture_id) else {
            if DEBUG_TRAX {
                get_trax().lock().put_stat(2);
            }
            panic!("unknown texture {gfx_texture_id}")
        };
        if DEBUG_GFXAPI {
            d!("Invoked method: delete_texture({gfx_texture_id} => {texture:?})");
        }
        self.ctx.delete_texture(texture);
        if DEBUG_TRAX {
            get_trax().lock().put_stat(0);
        }
    }
    pub(self) fn method_new_vertex_buffer(&mut self, verts: &[Vertex], gfx_buffer_id: BufferId) {
        let buffer = self.ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(verts),
        );
        if DEBUG_GFXAPI {
            d!("Invoked method: new_vertex_buffer(..., {gfx_buffer_id}) -> {buffer:?}");
            //debug!(target: "gfx", "Invoked method: new_vertex_buffer({:?}, {}) -> {:?}",
            //       verts, gfx_buffer_id, buffer);
        }
        if let Some(_) = self.buffers.insert(gfx_buffer_id, buffer) {
            if DEBUG_TRAX {
                get_trax().lock().put_stat(2);
            }
            panic!("Duplicate vertex buffer ID={gfx_buffer_id} detected!")
        }
        if DEBUG_TRAX {
            get_trax().lock().put_stat(0);
        }
    }
    pub(self) fn method_new_index_buffer(&mut self, indices: &[u16], gfx_buffer_id: BufferId) {
        let buffer = self.ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&indices),
        );
        if DEBUG_GFXAPI {
            d!("Invoked method: new_index_buffer({gfx_buffer_id}) -> {buffer:?}");
            //debug!(target: "gfx", "Invoked method: new_index_buffer({:?}, {}) -> {:?}",
            //       indices, gfx_buffer_id, buffer);
        }
        if let Some(_) = self.buffers.insert(gfx_buffer_id, buffer) {
            if DEBUG_TRAX {
                get_trax().lock().put_stat(2);
            }
            panic!("Duplicate index buffer ID={gfx_buffer_id} detected!")
        }
        if DEBUG_TRAX {
            get_trax().lock().put_stat(0);
        }
    }
    pub(self) fn method_delete_buffer(&mut self, gfx_buffer_id: BufferId) {
        let Some(buffer) = self.buffers.remove(&gfx_buffer_id) else {
            if DEBUG_TRAX {
                get_trax().lock().put_stat(2);
            }
            panic!("unknown buffer {gfx_buffer_id}");
        };
        if DEBUG_GFXAPI {
            d!("Invoked method: delete_buffer({gfx_buffer_id} => {buffer:?})");
        }
        self.ctx.delete_buffer(buffer);
        if DEBUG_TRAX {
            get_trax().lock().put_stat(0);
        }
    }
    pub(self) fn method_new_anim(&mut self, gfx_anim_id: AnimId, frames_len: usize, oneshot: bool) {
        if DEBUG_GFXAPI {
            d!("Invoked method: new_anim({gfx_anim_id}, {frames_len}, {oneshot})");
        }
        if let Some(_) = self.anims.insert(gfx_anim_id, GfxSeqAnim::new(frames_len, oneshot)) {
            panic!("Duplicate anim ID={gfx_anim_id} detected!");
        }
    }
    pub(self) fn method_update_anim(
        &mut self,
        gfx_anim_id: AnimId,
        frame_idx: usize,
        frame: AnimFrame,
    ) {
        let Some(anim) = self.anims.get_mut(&gfx_anim_id) else {
            panic!("couldn't find anim {gfx_anim_id}");
        };
        if DEBUG_GFXAPI {
            d!("Invoked method: update_anim({gfx_anim_id}[{frame_idx}] => {frame:?})");
        }
        anim.set(frame_idx, frame, &self.textures, &self.buffers);
    }
    pub(self) fn method_delete_anim(&mut self, gfx_anim_id: AnimId) {
        let Some(anim) = self.anims.remove(&gfx_anim_id) else {
            panic!("couldn't find anim {gfx_anim_id}");
        };
        if DEBUG_GFXAPI {
            d!("Invoked method: delete_anim({} => {:?})", gfx_anim_id, anim);
        }
    }
    fn method_replace_draw_calls(&mut self, batch_timest: Timestamp, dcs: Vec<(DcId, DrawCall)>) {
        if DEBUG_GFXAPI {
            d!("Invoked method: replace_draw_calls({:?})", dcs);
        }

        // Phase 1: Check for conflicts with newer batches
        // If any draw call in this batch belongs to an older batch than the existing one,
        // reject the entire batch to maintain atomicity
        for (key, _) in &dcs {
            if let Some(old_val) = self.draw_calls.get(key) {
                if old_val.timest > batch_timest {
                    // Entire batch is stale, reject all
                    t!("Rejected stale batch {batch_timest}: conflict with newer batch {} on {key}", old_val.timest);
                    if DEBUG_TRAX {
                        get_trax().lock().put_stat(3); // New stat: rejected batch
                    }
                    return;
                }
            }
        }

        // Phase 2: Apply entire batch atomically
        self.apply_draw_calls(batch_timest, dcs)
    }
    pub(self) fn apply_draw_calls(&mut self, batch_timest: Timestamp, dcs: Vec<(DcId, DrawCall)>) {
        for (key, val) in dcs {
            let val = val.compile(&self.textures, &self.buffers, batch_timest);

            // Insert/replace draw call
            self.draw_calls.insert(key, val);

            if DEBUG_TRAX {
                get_trax().lock().put_stat(1); // Success
            }
        }
    }

    fn trax_method(&self, epoch: EpochIndex, method: &GraphicsMethod) {
        let mut trax = get_trax().lock();
        match method {
            GraphicsMethod::NewTexture((_, _, _, _, gtex_id, tag)) => {
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
            GraphicsMethod::NewSeqAnim { .. } => {
                //trax.put_idxs(epoch, idxs.clone(), *gbuff_id, *tag, 1);
            }
            GraphicsMethod::UpdateSeqAnim { .. } => {
                //trax.put_idxs(epoch, idxs.clone(), *gbuff_id, *tag, 1);
            }
            GraphicsMethod::DeleteSeqAnim(..) => {
                //trax.del_buf(epoch, *gbuff_id, *tag, *buftype);
            }
            GraphicsMethod::ReplaceGfxDrawCalls { batch_id, dcs } => {
                if let Some(bid) = batch_id {
                    trax.put_dcs(epoch, *bid, dcs);
                }
            }
            GraphicsMethod::StartBatch { batch_id, tag } => {
                trax.put_start_batch(epoch, *batch_id, *tag);
            }
            GraphicsMethod::EndBatch { batch_id, timest: _ } => {
                trax.put_end_batch(epoch, *batch_id);
            }
            GraphicsMethod::Noop => panic!("noop"),
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

    #[instrument(skip_all, target = "gfx::process")]
    fn process_methods(&mut self) {
        // Process as many methods as we can
        while let Ok((epoch, method)) = self.method_recv.try_recv() {
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
                t!(
                    "Discard method with old epoch: {epoch} curr: {} [method={method:?}]",
                    self.epoch
                );
                continue
            }
            assert_eq!(epoch, self.epoch);
            self.process_method(method);
            if DEBUG_TRAX {
                get_trax().lock().flush();
            }
        }
    }

    #[instrument(skip_all, target = "gfx::pruner")]
    fn prime_screen(&mut self) {
        let methods = self.pruner.recv_all();
        assert!(self.pending_batches.is_empty());
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
                GraphicsMethod::NewTexture(_) |
                GraphicsMethod::DeleteTexture(_) |
                GraphicsMethod::NewVertexBuffer(_) |
                GraphicsMethod::NewIndexBuffer(_) |
                GraphicsMethod::DeleteBuffer(_) |
                GraphicsMethod::NewSeqAnim { .. } |
                GraphicsMethod::UpdateSeqAnim { .. } |
                GraphicsMethod::DeleteSeqAnim(_) => self.process_method(method),

                GraphicsMethod::ReplaceGfxDrawCalls { .. } |
                GraphicsMethod::StartBatch { .. } |
                GraphicsMethod::EndBatch { .. } => {
                    panic!("unsupported pruned methods should be dropped!")
                }

                GraphicsMethod::Noop => panic!("noop"),
            }
            if DEBUG_TRAX {
                get_trax().lock().flush();
            }
        }

        // Trigger a full screen redraw by sending a resize event
        let (width, height) = miniquad::window::screen_size();
        self.event_pub.notify_resize(Dimension::from([width, height]));
    }

    fn close_pending_batches(&mut self) {
        // Immediately apply any pending batches when the screen is switched off
        let batch_ids: Vec<_> = self.pending_batches.keys().cloned().collect();
        if !batch_ids.is_empty() {
            t!("Force closing pending batches: {batch_ids:?}");
        }

        for batch_id in batch_ids {
            self.process_method(GraphicsMethod::EndBatch { batch_id, timest: unixtime() });
            if !self.dropped_batches.insert(batch_id) {
                panic!("dropped batch {batch_id} already exits!");
            }
        }
        assert!(self.pending_batches.is_empty());
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum ScreenState {
    // Screen is on as normal
    On,
    // First update since screen has gone off
    SwitchOff,
    // Subsequent updates after SwitchOff state
    Off,
    // First update when screen is on but before draw has been called
    ReadyOn,
    // First update and draw has been called
    PrimedOn,
    // Second update ready to process pruned buffered allocs
    SwitchOn,
}

impl ScreenState {
    fn update(&mut self, egl_is_disabled: bool) {
        use ScreenState::*;
        *self = if egl_is_disabled {
            match self {
                On | ReadyOn | PrimedOn | SwitchOn => SwitchOff,
                SwitchOff => Off,
                Off => Off,
            }
        } else {
            match self {
                SwitchOff | Off => ReadyOn,
                ReadyOn => PrimedOn,
                PrimedOn => SwitchOn,
                SwitchOn => On,
                On => On,
            }
        };
    }
}

impl EventHandler for Stage {
    fn update(&mut self) {
        // todo: trax is all messed up in this func

        let old_screen_state = self.screen_state;
        self.screen_state.update(self.egl_ctx_is_disabled());
        if self.screen_state != old_screen_state {
            d!("Switching screen state {old_screen_state:?} => {:?}", self.screen_state);
        }

        match self.screen_state {
            ScreenState::SwitchOff => {
                self.close_pending_batches();

                // Screen is off so collect all methods into the pruner
                self.pruner.drain(&self.method_recv);
            }
            ScreenState::Off => {
                assert!(self.pending_batches.is_empty());

                // Screen is off so collect all methods into the pruner
                self.pruner.drain(&self.method_recv);
            }
            ScreenState::ReadyOn => {
                // We actually want to skip draining the prune queue the first time so
                // miniquad draw() actually gets a chance to be called first.
                // Otherwise we will see a black screen for a sec or so while update() is running.
            }
            ScreenState::PrimedOn => {
                self.prime_screen();
            }
            ScreenState::SwitchOn => {
                // This should have been cleared in previous PrimedOn state
                let methods = self.pruner.recv_all();
                assert!(methods.is_empty());

                self.process_methods();
            }
            ScreenState::On => {
                self.process_methods();
            }
        }
    }

    fn draw(&mut self) {
        self.ctx.begin_default_pass(PassAction::clear_color(0., 0., 0., 1.));

        // Apply default RGB pipeline
        self.ctx.apply_pipeline(&self.loaded_pipelines[GraphicPipeline::RGB as usize]);

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
            loaded_pipelines: &self.loaded_pipelines,
            scale: 1.,
            view: Rectangle::from([0., 0., screen_w, screen_h]),
            cursor: Point::zero(),
            gfx_pipeline: GraphicPipeline::RGB,
            anims: &mut self.anims,
            overlays: vec![],
        };
        render_ctx.draw();

        self.ctx.end_render_pass();
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

        // Lazy cache window on first touch event
        if self.window_node.is_none() {
            let god = GOD.get().unwrap();
            self.window_node = god.app.sg_root.lookup_node("/window");
        }

        // Clone window_node to avoid borrow conflict with RenderApiSync
        let window_node = self.window_node.clone();

        // Create RenderApiSync for direct graphics operations
        let mut render_api_sync = RenderApiSync::new(self);

        // Direct call to Window's handle_touch_event_sync
        if let Some(window_node) = &window_node {
            match window_node.pimpl() {
                Pimpl::Window(win) => {
                    if win.handle_touch_sync(&mut render_api_sync, phase, id, pos) {
                        return
                    }
                }
                _ => panic!(),
            }
        }

        drop(render_api_sync);

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
            #[cfg(target_os = "android")]
            sleep_interval_ms: Some(40),
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
