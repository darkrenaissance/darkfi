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

use darkfi::system::CondVar;
use darkfi_serial::{async_trait, Decodable, Encodable, SerialDecodable, SerialEncodable};
use futures::AsyncWriteExt;
use log::debug;
use miniquad::{
    conf, window, Backend, Bindings, BlendFactor, BlendState, BlendValue, BufferLayout,
    BufferSource, BufferType, BufferUsage, Equation, EventHandler, KeyCode, KeyMods, MouseButton,
    PassAction, Pipeline, PipelineParams, RenderingBackend, ShaderMeta, ShaderSource, TouchPhase,
    UniformDesc, UniformType, VertexAttribute, VertexFormat,
};
use std::{
    collections::HashMap,
    fs::File,
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc, Arc, Mutex as SyncMutex,
    },
    time::{Duration, Instant},
};

mod favico;
mod linalg;
pub use linalg::{Dimension, Point, Rectangle};
mod shader;

use crate::{
    app::AppPtr,
    error::{Error, Result},
    pubsub::{Publisher, PublisherPtr, Subscription, SubscriptionId},
    util::{ansi_texture, AsyncRuntime},
    GOD,
};

// This is very noisy so suppress output by default
const DEBUG_RENDER: bool = false;
const DEBUG_GFXAPI: bool = false;

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

pub type GfxTextureId = u32;
pub type GfxBufferId = u32;

static NEXT_BUFFER_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_TEXTURE_ID: AtomicU32 = AtomicU32::new(0);

pub type ManagedTexturePtr = Arc<ManagedTexture>;

/// Auto-deletes texture on drop
#[derive(Clone)]
pub struct ManagedTexture {
    id: GfxTextureId,
    render_api: RenderApi,
}

impl Drop for ManagedTexture {
    fn drop(&mut self) {
        self.render_api.delete_unmanaged_texture(self.id);
    }
}

impl std::fmt::Debug for ManagedTexture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedTexture").field("id", &self.id).finish()
    }
}

pub type ManagedBufferPtr = Arc<ManagedBuffer>;

/// Auto-deletes buffer on drop
#[derive(Clone)]
pub struct ManagedBuffer {
    id: GfxBufferId,
    render_api: RenderApi,
}

impl Drop for ManagedBuffer {
    fn drop(&mut self) {
        self.render_api.delete_unmanaged_buffer(self.id);
    }
}

impl std::fmt::Debug for ManagedBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedBuffer").field("id", &self.id).finish()
    }
}

pub type EpochIndex = u32;

#[derive(Clone)]
pub struct RenderApi {
    /// We are abusing async_channel since it's cloneable whereas std::sync::mpsc is shit.
    method_req: async_channel::Sender<(EpochIndex, GraphicsMethod)>,
    /// Keep track of the current UI epoch
    epoch: Arc<AtomicU32>,
}

impl RenderApi {
    pub fn new(method_req: async_channel::Sender<(EpochIndex, GraphicsMethod)>) -> Self {
        Self { method_req, epoch: Arc::new(AtomicU32::new(0)) }
    }

    fn next_epoch(&self) -> EpochIndex {
        self.epoch.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn send(&self, method: GraphicsMethod) {
        let epoch = self.epoch.load(Ordering::Relaxed);
        let _ = self.method_req.try_send((epoch, method)).unwrap();
    }

    fn new_unmanaged_texture(&self, width: u16, height: u16, data: Vec<u8>) -> GfxTextureId {
        let gfx_texture_id = NEXT_TEXTURE_ID.fetch_add(1, Ordering::SeqCst);

        let method = GraphicsMethod::NewTexture((width, height, data, gfx_texture_id));
        self.send(method);

        gfx_texture_id
    }

    pub fn new_texture(&self, width: u16, height: u16, data: Vec<u8>) -> ManagedTexturePtr {
        Arc::new(ManagedTexture {
            id: self.new_unmanaged_texture(width, height, data),
            render_api: self.clone(),
        })
    }

    fn delete_unmanaged_texture(&self, texture: GfxTextureId) {
        let method = GraphicsMethod::DeleteTexture(texture);
        self.send(method);
    }

    fn new_unmanaged_vertex_buffer(&self, verts: Vec<Vertex>) -> GfxBufferId {
        let gfx_buffer_id = NEXT_BUFFER_ID.fetch_add(1, Ordering::SeqCst);

        let method = GraphicsMethod::NewVertexBuffer((verts, gfx_buffer_id));
        self.send(method);

        gfx_buffer_id
    }

    fn new_unmanaged_index_buffer(&self, indices: Vec<u16>) -> GfxBufferId {
        let gfx_buffer_id = NEXT_BUFFER_ID.fetch_add(1, Ordering::SeqCst);

        let method = GraphicsMethod::NewIndexBuffer((indices, gfx_buffer_id));
        self.send(method);

        gfx_buffer_id
    }

    pub fn new_vertex_buffer(&self, verts: Vec<Vertex>) -> ManagedBufferPtr {
        Arc::new(ManagedBuffer {
            id: self.new_unmanaged_vertex_buffer(verts),
            render_api: self.clone(),
        })
    }
    pub fn new_index_buffer(&self, indices: Vec<u16>) -> ManagedBufferPtr {
        Arc::new(ManagedBuffer {
            id: self.new_unmanaged_index_buffer(indices),
            render_api: self.clone(),
        })
    }

    fn delete_unmanaged_buffer(&self, buffer: GfxBufferId) {
        let method = GraphicsMethod::DeleteBuffer(buffer);
        self.send(method);
    }

    pub fn replace_draw_calls(&self, timest: u64, dcs: Vec<(u64, GfxDrawCall)>) {
        let method = GraphicsMethod::ReplaceDrawCalls { timest, dcs };
        self.send(method);
    }
}

#[derive(Clone, Debug)]
pub struct GfxDrawMesh {
    pub vertex_buffer: ManagedBufferPtr,
    pub index_buffer: ManagedBufferPtr,
    pub texture: Option<ManagedTexturePtr>,
    pub num_elements: i32,
}

impl GfxDrawMesh {
    fn compile(
        self,
        textures: &HashMap<GfxTextureId, miniquad::TextureId>,
        buffers: &HashMap<GfxBufferId, miniquad::BufferId>,
    ) -> Option<DrawMesh> {
        let vertex_buffer_id = self.vertex_buffer.id;
        let index_buffer_id = self.index_buffer.id;
        let buffers_keep_alive = [self.vertex_buffer, self.index_buffer];
        let texture = match self.texture {
            Some(gfx_texture) => Self::try_get_texture(textures, gfx_texture),
            None => None,
        };
        Some(DrawMesh {
            vertex_buffer: Self::try_get_buffer(buffers, vertex_buffer_id)?,
            index_buffer: Self::try_get_buffer(buffers, index_buffer_id)?,
            buffers_keep_alive,
            texture,
            num_elements: self.num_elements,
        })
    }

    fn try_get_texture(
        textures: &HashMap<GfxTextureId, miniquad::TextureId>,
        gfx_texture: ManagedTexturePtr,
    ) -> Option<(ManagedTexturePtr, miniquad::TextureId)> {
        let gfx_texture_id = gfx_texture.id;

        let Some(mq_texture_id) = textures.get(&gfx_texture_id) else {
            error!(target: "gfx", "Serious error: missing texture ID={gfx_texture_id}");
            error!(target: "gfx", "Dumping textures:");
            for (gfx_texture_id, texture_id) in textures {
                error!(target: "gfx", "{gfx_texture_id} => {texture_id:?}");
            }

            panic!("Missing texture ID={gfx_texture_id}");
            return None
        };

        Some((gfx_texture, textures[&gfx_texture_id]))
    }

    fn try_get_buffer(
        buffers: &HashMap<GfxBufferId, miniquad::BufferId>,
        gfx_buffer_id: GfxBufferId,
    ) -> Option<miniquad::BufferId> {
        let Some(mq_buffer_id) = buffers.get(&gfx_buffer_id) else {
            error!(target: "gfx", "Serious error: missing buffer ID={gfx_buffer_id}");
            error!(target: "gfx", "Dumping buffers:");
            for (gfx_buffer_id, buffer_id) in buffers {
                error!(target: "gfx", "{gfx_buffer_id} => {buffer_id:?}");
            }

            panic!("Missing buffer ID={gfx_buffer_id}");
            return None
        };
        Some(*mq_buffer_id)
    }
}

#[derive(Debug, Clone)]
pub enum GfxDrawInstruction {
    SetScale(f32),
    Move(Point),
    SetPos(Point),
    ApplyView(Rectangle),
    Draw(GfxDrawMesh),
    EnableDebug,
}

impl GfxDrawInstruction {
    fn compile(
        self,
        textures: &HashMap<GfxTextureId, miniquad::TextureId>,
        buffers: &HashMap<GfxBufferId, miniquad::BufferId>,
    ) -> Option<DrawInstruction> {
        let instr = match self {
            Self::SetScale(scale) => DrawInstruction::SetScale(scale),
            Self::Move(off) => DrawInstruction::Move(off),
            Self::SetPos(pos) => DrawInstruction::SetPos(pos),
            Self::ApplyView(view) => DrawInstruction::ApplyView(view),
            Self::Draw(mesh) => DrawInstruction::Draw(mesh.compile(textures, buffers)?),
            Self::EnableDebug => DrawInstruction::EnableDebug,
        };
        Some(instr)
    }
}

#[derive(Clone, Debug, Default)]
pub struct GfxDrawCall {
    pub instrs: Vec<GfxDrawInstruction>,
    pub dcs: Vec<u64>,
    pub z_index: u32,
}

impl GfxDrawCall {
    fn compile(
        self,
        textures: &HashMap<GfxTextureId, miniquad::TextureId>,
        buffers: &HashMap<GfxBufferId, miniquad::BufferId>,
        timest: u64,
    ) -> Option<DrawCall> {
        Some(DrawCall {
            instrs: self
                .instrs
                .into_iter()
                .map(|i| i.compile(textures, buffers))
                .collect::<Option<Vec<_>>>()?,
            dcs: self.dcs,
            z_index: self.z_index,
            timest,
        })
    }
}

#[derive(Clone, Debug)]
struct DrawMesh {
    vertex_buffer: miniquad::BufferId,
    index_buffer: miniquad::BufferId,
    /// Keeps the buffers alive for the duration of this draw call
    buffers_keep_alive: [ManagedBufferPtr; 2],
    texture: Option<(ManagedTexturePtr, miniquad::TextureId)>,
    num_elements: i32,
}

#[derive(Debug, Clone)]
enum DrawInstruction {
    SetScale(f32),
    Move(Point),
    SetPos(Point),
    ApplyView(Rectangle),
    Draw(DrawMesh),
    EnableDebug,
}

#[derive(Debug)]
struct DrawCall {
    instrs: Vec<DrawInstruction>,
    dcs: Vec<u64>,
    z_index: u32,
    timest: u64,
}

struct RenderContext<'a> {
    ctx: &'a mut Box<dyn RenderingBackend>,
    draw_calls: &'a HashMap<u64, DrawCall>,
    uniforms_data: [u8; 128],
    white_texture: miniquad::TextureId,

    scale: f32,
    view: Rectangle,
    cursor: Point,
}

impl<'a> RenderContext<'a> {
    fn draw(&mut self) {
        if DEBUG_RENDER {
            debug!(target: "gfx", "RenderContext::draw()");
        }
        let curr_pos = Point::zero();
        self.draw_call(&self.draw_calls[&0], 0, DEBUG_RENDER);
        if DEBUG_RENDER {
            debug!(target: "gfx", "RenderContext::draw() [DONE]");
        }
    }

    fn apply_view(&mut self) {
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

    fn draw_call(&mut self, draw_call: &DrawCall, mut indent: u32, mut is_debug: bool) {
        let mut ws = if is_debug { " ".repeat(indent as usize * 4) } else { String::new() };

        let old_scale = self.scale;
        let old_view = self.view;
        let old_cursor = self.cursor;

        for instr in &draw_call.instrs {
            match instr {
                DrawInstruction::SetScale(scale) => {
                    self.scale = *scale;
                    if is_debug {
                        debug!(target: "gfx", "{ws}set_scale({scale})");
                    }
                }
                DrawInstruction::Move(off) => {
                    self.cursor += *off;
                    if is_debug {
                        debug!(target: "gfx",
                            "{ws}move({off:?})  cursor={:?}, scale={}, view={:?}",
                            self.cursor, self.scale, self.view
                        );
                    }
                    self.apply_model();
                }
                DrawInstruction::SetPos(pos) => {
                    self.cursor = old_cursor + *pos;
                    if is_debug {
                        debug!(target: "gfx",
                            "{ws}set_pos({pos:?})  cursor={:?}, scale={}, view={:?}",
                            self.cursor, self.scale, self.view
                        );
                    }
                    self.apply_model();
                }
                DrawInstruction::ApplyView(view) => {
                    // Adjust view relative to cursor
                    self.view = *view + self.cursor;
                    // We could just skip drawing when clipping rect isn't visible
                    // using an is_visible flag.
                    match self.view.clip(&old_view) {
                        Some(clipped) => self.view = clipped,
                        None => self.view = Rectangle::zero()
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
                DrawInstruction::Draw(mesh) => {
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
                DrawInstruction::EnableDebug => {
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

#[derive(Clone, Debug)]
pub enum GraphicsMethod {
    NewTexture((u16, u16, Vec<u8>, GfxTextureId)),
    DeleteTexture(GfxTextureId),
    NewVertexBuffer((Vec<Vertex>, GfxBufferId)),
    NewIndexBuffer((Vec<u16>, GfxBufferId)),
    DeleteBuffer(GfxBufferId),
    ReplaceDrawCalls { timest: u64, dcs: Vec<(u64, GfxDrawCall)> },
}

pub type GraphicsEventPublisherPtr = Arc<GraphicsEventPublisher>;

pub struct GraphicsEventPublisher {
    resize: PublisherPtr<Dimension>,
    key_down: PublisherPtr<(KeyCode, KeyMods, bool)>,
    key_up: PublisherPtr<(KeyCode, KeyMods)>,
    chr: PublisherPtr<(char, KeyMods, bool)>,
    mouse_btn_down: PublisherPtr<(MouseButton, Point)>,
    mouse_btn_up: PublisherPtr<(MouseButton, Point)>,
    mouse_move: PublisherPtr<Point>,
    mouse_wheel: PublisherPtr<Point>,
    touch: PublisherPtr<(TouchPhase, u64, Point)>,
}

impl GraphicsEventPublisher {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            resize: Publisher::new(),
            key_down: Publisher::new(),
            key_up: Publisher::new(),
            chr: Publisher::new(),
            mouse_btn_down: Publisher::new(),
            mouse_btn_up: Publisher::new(),
            mouse_move: Publisher::new(),
            mouse_wheel: Publisher::new(),
            touch: Publisher::new(),
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

    pub fn subscribe_resize(&self) -> Subscription<Dimension> {
        self.resize.clone().subscribe()
    }
    pub fn subscribe_key_down(&self) -> Subscription<(KeyCode, KeyMods, bool)> {
        self.key_down.clone().subscribe()
    }
    pub fn subscribe_key_up(&self) -> Subscription<(KeyCode, KeyMods)> {
        self.key_up.clone().subscribe()
    }
    pub fn subscribe_char(&self) -> Subscription<(char, KeyMods, bool)> {
        self.chr.clone().subscribe()
    }
    pub fn subscribe_mouse_btn_down(&self) -> Subscription<(MouseButton, Point)> {
        self.mouse_btn_down.clone().subscribe()
    }
    pub fn subscribe_mouse_btn_up(&self) -> Subscription<(MouseButton, Point)> {
        self.mouse_btn_up.clone().subscribe()
    }
    pub fn subscribe_mouse_move(&self) -> Subscription<Point> {
        self.mouse_move.clone().subscribe()
    }
    pub fn subscribe_mouse_wheel(&self) -> Subscription<Point> {
        self.mouse_wheel.clone().subscribe()
    }
    pub fn subscribe_touch(&self) -> Subscription<(TouchPhase, u64, Point)> {
        self.touch.clone().subscribe()
    }
}

struct Stage {
    ctx: Box<dyn RenderingBackend>,
    pipeline: Pipeline,
    white_texture: miniquad::TextureId,
    draw_calls: HashMap<u64, DrawCall>,

    textures: HashMap<GfxTextureId, miniquad::TextureId>,
    buffers: HashMap<GfxBufferId, miniquad::BufferId>,

    epoch: EpochIndex,
    method_rep: async_channel::Receiver<(EpochIndex, GraphicsMethod)>,
    event_pub: GraphicsEventPublisherPtr,
}

impl Stage {
    pub fn new() -> Self {
        let mut ctx: Box<dyn RenderingBackend> = window::new_rendering_backend();

        // This will start the app to start. Needed since we cannot get window size for init
        // until window is created.
        let god = GOD.get().unwrap();
        god.start_app();

        // Start a new epoch. This is a brand new UI run.
        let epoch = god.render_api.next_epoch();

        let method_rep = god.method_rep.clone();
        let event_pub = god.event_pub.clone();
        drop(god);

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

        Stage {
            ctx,
            pipeline,
            white_texture,
            draw_calls: HashMap::from([(
                0,
                DrawCall { instrs: vec![], dcs: vec![], z_index: 0, timest: 0 },
            )]),

            textures: HashMap::new(),
            buffers: HashMap::new(),

            epoch,
            method_rep,
            event_pub,
        }
    }

    fn process_method(&mut self, method: GraphicsMethod) {
        //debug!(target: "gfx", "Received method: {:?}", method);
        match method {
            GraphicsMethod::NewTexture((width, height, data, gfx_texture_id)) => {
                self.method_new_texture(width, height, data, gfx_texture_id)
            }
            GraphicsMethod::DeleteTexture(texture) => self.method_delete_texture(texture),
            GraphicsMethod::NewVertexBuffer((verts, sendr)) => {
                self.method_new_vertex_buffer(verts, sendr)
            }
            GraphicsMethod::NewIndexBuffer((indices, sendr)) => {
                self.method_new_index_buffer(indices, sendr)
            }
            GraphicsMethod::DeleteBuffer(buffer) => self.method_delete_buffer(buffer),
            GraphicsMethod::ReplaceDrawCalls { timest, dcs } => {
                self.method_replace_draw_calls(timest, dcs)
            }
        };
    }

    fn method_new_texture(
        &mut self,
        width: u16,
        height: u16,
        data: Vec<u8>,
        gfx_texture_id: GfxTextureId,
    ) {
        let texture = self.ctx.new_texture_from_rgba8(width, height, &data);
        if DEBUG_GFXAPI {
            debug!(target: "gfx", "Invoked method: new_texture({}, {}, ..., {}) -> {:?}",
                   width, height, gfx_texture_id, texture);
            //debug!(target: "gfx", "Invoked method: new_texture({}, {}, ..., {}) -> {:?}\n{}",
            //       width, height, gfx_texture_id, texture,
            //       ansi_texture(width as usize, height as usize, &data));
        }
        if let Some(_) = self.textures.insert(gfx_texture_id, texture) {
            panic!("Duplicate texture ID={gfx_texture_id} detected!");
        }
    }
    fn method_delete_texture(&mut self, gfx_texture_id: GfxTextureId) {
        let texture = self.textures.remove(&gfx_texture_id).expect("couldn't find gfx_texture_id");
        if DEBUG_GFXAPI {
            debug!(target: "gfx", "Invoked method: delete_texture({} => {:?})",
                   gfx_texture_id, texture);
        }
        self.ctx.delete_texture(texture);
    }
    fn method_new_vertex_buffer(&mut self, verts: Vec<Vertex>, gfx_buffer_id: GfxBufferId) {
        let buffer = self.ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&verts),
        );
        if DEBUG_GFXAPI {
            debug!(target: "gfx", "Invoked method: new_vertex_buffer(..., {}) -> {:?}",
                   gfx_buffer_id, buffer);
            //debug!(target: "gfx", "Invoked method: new_vertex_buffer({:?}, {}) -> {:?}",
            //       verts, gfx_buffer_id, buffer);
        }
        if let Some(_) = self.buffers.insert(gfx_buffer_id, buffer) {
            panic!("Duplicate vertex buffer ID={gfx_buffer_id} detected!");
        }
    }
    fn method_new_index_buffer(&mut self, indices: Vec<u16>, gfx_buffer_id: GfxBufferId) {
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
            panic!("Duplicate index buffer ID={gfx_buffer_id} detected!");
        }
    }
    fn method_delete_buffer(&mut self, gfx_buffer_id: GfxBufferId) {
        let buffer = self.buffers.remove(&gfx_buffer_id).expect("couldn't find gfx_buffer_id");
        if DEBUG_GFXAPI {
            debug!(target: "gfx", "Invoked method: delete_buffer({} => {:?})",
                   gfx_buffer_id, buffer);
        }
        self.ctx.delete_buffer(buffer);
    }
    fn method_replace_draw_calls(&mut self, timest: u64, dcs: Vec<(u64, GfxDrawCall)>) {
        if DEBUG_GFXAPI {
            debug!(target: "gfx", "Invoked method: replace_draw_calls({:?})", dcs);
        }
        for (key, val) in dcs {
            let Some(val) = val.compile(&self.textures, &self.buffers, timest) else {
                error!(target: "gfx", "fatal: replace_draw_calls({timest}, ...) failed with item ID={key}");
                continue
            };
            //self.draw_calls.insert(key, val);
            match self.draw_calls.get_mut(&key) {
                Some(old_val) => {
                    // Only replace the draw call if it is more recent
                    if old_val.timest < timest {
                        *old_val = val;
                    } else {
                        trace!(target: "gfx", "Rejected stale draw_call {key}: {val:?}");
                    }
                }
                None => {
                    self.draw_calls.insert(key, val);
                }
            }
        }
    }
}

impl EventHandler for Stage {
    fn update(&mut self) {
        // Process as many methods as we can
        while let Ok((epoch, method)) = self.method_rep.try_recv() {
            if epoch < self.epoch {
                // Discard old rubbish
                trace!(target: "gfx", "Discard method with old epoch: {epoch} curr: {} [method={method:?}]", self.epoch);
                continue
            }
            assert_eq!(epoch, self.epoch);
            self.process_method(method);
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

        let mut render_ctx = RenderContext {
            ctx: &mut self.ctx,
            draw_calls: &self.draw_calls,
            uniforms_data,
            white_texture: self.white_texture,
            scale: 1.,
            view: Rectangle::from([0., 0., screen_w, screen_h]),
            cursor: Point::from([0., 0.]),
        };
        render_ctx.draw();

        self.ctx.commit_frame();
    }

    fn resize_event(&mut self, width: f32, height: f32) {
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

pub fn run_gui() {
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
            linux_backend: miniquad::conf::LinuxBackend::WaylandWithX11Fallback,
            //blocking_event_loop: true,
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
