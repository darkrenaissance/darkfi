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

use darkfi_serial::{async_trait, SerialDecodable, SerialEncodable};
use log::debug;
use miniquad::{
    conf, window, Backend, Bindings, BlendFactor, BlendState, BlendValue, BufferLayout,
    BufferSource, BufferType, BufferUsage, Equation, EventHandler, KeyCode, KeyMods, MouseButton,
    PassAction, Pipeline, PipelineParams, RenderingBackend, ShaderMeta, ShaderSource, TouchPhase,
    UniformDesc, UniformType, VertexAttribute, VertexFormat,
};
use std::{
    collections::HashMap,
    sync::{mpsc, Arc, Mutex as SyncMutex},
    time::{Duration, Instant},
};

mod shader;

use crate::{
    app::{AppPtr, AsyncRuntime},
    error::{Error, Result},
    pubsub::{Publisher, PublisherPtr, Subscription, SubscriptionId},
    util::ansi_texture,
};

pub type GfxTextureId = u32;
pub type GfxBufferId = u32;

// This is very noisy so suppress output by default
const DEBUG_RENDER: bool = false;
const DEBUG_GFXAPI: bool = false;

#[derive(Debug, SerialEncodable, SerialDecodable)]
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

#[derive(Clone, Debug)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn unpack(&self) -> (f32, f32) {
        (self.x, self.y)
    }

    pub fn as_arr(&self) -> [f32; 2] {
        [self.x, self.y]
    }

    pub fn offset(&self, off_x: f32, off_y: f32) -> Self {
        Self { x: self.x + off_x, y: self.y + off_y }
    }

    pub fn to_rect(&self, w: f32, h: f32) -> Rectangle {
        Rectangle { x: self.x, y: self.y, w, h }
    }
}

impl From<[f32; 2]> for Point {
    fn from(pos: [f32; 2]) -> Self {
        Self { x: pos[0], y: pos[1] }
    }
}

#[derive(Debug, Clone)]
pub struct Rectangle {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rectangle {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn zero() -> Self {
        Self { x: 0., y: 0., w: 0., h: 0. }
    }

    pub fn from_array(arr: [f32; 4]) -> Self {
        Self { x: arr[0], y: arr[1], w: arr[2], h: arr[3] }
    }

    pub fn clip(&self, other: &Self) -> Option<Self> {
        if other.x + other.w < self.x ||
            other.x > self.x + self.w ||
            other.y + other.h < self.y ||
            other.y > self.y + self.h
        {
            return None
        }

        let mut clipped = other.clone();
        if clipped.x < self.x {
            clipped.x = self.x;
            clipped.w = other.x + other.w - clipped.x;
        }
        if clipped.y < self.y {
            clipped.y = self.y;
            clipped.h = other.y + other.h - clipped.y;
        }
        if clipped.x + clipped.w > self.x + self.w {
            clipped.w = self.x + self.w - clipped.x;
        }
        if clipped.y + clipped.h > self.y + self.h {
            clipped.h = self.y + self.h - clipped.y;
        }
        Some(clipped)
    }

    pub fn clip_point(&self, point: &mut Point) {
        if point.x < self.x {
            point.x = self.x;
        }
        if point.y < self.y {
            point.y = self.y;
        }
        if point.x > self.x + self.w {
            point.x = self.x + self.w;
        }
        if point.y > self.y + self.h {
            point.y = self.y + self.h;
        }
    }

    pub fn contains(&self, point: &Point) -> bool {
        self.x <= point.x &&
            point.x <= self.x + self.w &&
            self.y <= point.y &&
            point.y <= self.y + self.h
    }

    pub fn rhs(&self) -> f32 {
        self.x + self.w
    }
    pub fn bhs(&self) -> f32 {
        self.y + self.h
    }

    pub fn top_left(&self) -> Point {
        Point { x: self.x, y: self.y }
    }
    pub fn bottom_right(&self) -> Point {
        Point { x: self.x + self.w, y: self.y + self.h }
    }

    pub fn includes(&self, child: &Self) -> bool {
        self.contains(&child.top_left()) && self.contains(&child.bottom_right())
    }
}

pub type RenderApiPtr = Arc<RenderApi>;

pub struct RenderApi {
    method_req: mpsc::Sender<GraphicsMethod>,
}

impl RenderApi {
    pub fn new(method_req: mpsc::Sender<GraphicsMethod>) -> Arc<Self> {
        Arc::new(Self { method_req })
    }

    pub fn new_texture(&self, width: u16, height: u16, data: Vec<u8>) -> GfxTextureId {
        let gfx_texture_id = rand::random();

        let method = GraphicsMethod::NewTexture((width, height, data, gfx_texture_id));
        let _ = self.method_req.send(method);

        gfx_texture_id
    }

    pub fn delete_texture(&self, texture: GfxTextureId) {
        let method = GraphicsMethod::DeleteTexture(texture);
        let _ = self.method_req.send(method);
    }

    pub fn new_vertex_buffer(&self, verts: Vec<Vertex>) -> GfxBufferId {
        let gfx_buffer_id = rand::random();

        let method = GraphicsMethod::NewVertexBuffer((verts, gfx_buffer_id));
        let _ = self.method_req.send(method);

        gfx_buffer_id
    }

    pub fn new_index_buffer(&self, indices: Vec<u16>) -> GfxBufferId {
        let gfx_buffer_id = rand::random();

        let method = GraphicsMethod::NewIndexBuffer((indices, gfx_buffer_id));
        let _ = self.method_req.send(method);

        gfx_buffer_id
    }

    pub fn delete_buffer(&self, buffer: GfxBufferId) {
        let method = GraphicsMethod::DeleteBuffer(buffer);
        let _ = self.method_req.send(method);
    }

    pub fn replace_draw_calls(&self, dcs: Vec<(u64, GfxDrawCall)>) {
        let method = GraphicsMethod::ReplaceDrawCalls(dcs);
        let _ = self.method_req.send(method);
    }
}

#[derive(Clone, Debug)]
pub struct GfxDrawMesh {
    pub vertex_buffer: GfxBufferId,
    pub index_buffer: GfxBufferId,
    pub texture: Option<GfxTextureId>,
    pub num_elements: i32,
}

impl GfxDrawMesh {
    fn compile(
        self,
        textures: &HashMap<GfxTextureId, miniquad::TextureId>,
        buffers: &HashMap<GfxBufferId, miniquad::BufferId>,
    ) -> DrawMesh {
        DrawMesh {
            vertex_buffer: buffers[&self.vertex_buffer],
            index_buffer: buffers[&self.index_buffer],
            texture: self.texture.map(|t| textures[&t]),
            num_elements: self.num_elements,
        }
    }
}

#[derive(Debug, Clone)]
pub enum GfxDrawInstruction {
    ApplyViewport(Rectangle),
    ApplyMatrix(glam::Mat4),
    Draw(GfxDrawMesh),
}

impl GfxDrawInstruction {
    fn compile(
        self,
        textures: &HashMap<GfxTextureId, miniquad::TextureId>,
        buffers: &HashMap<GfxBufferId, miniquad::BufferId>,
    ) -> DrawInstruction {
        match self {
            Self::ApplyViewport(rect) => DrawInstruction::ApplyViewport(rect),
            Self::ApplyMatrix(mat) => DrawInstruction::ApplyMatrix(mat),
            Self::Draw(mesh) => DrawInstruction::Draw(mesh.compile(textures, buffers)),
        }
    }
}

#[derive(Debug)]
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
    ) -> DrawCall {
        DrawCall {
            instrs: self.instrs.into_iter().map(|i| i.compile(textures, buffers)).collect(),
            dcs: self.dcs,
            z_index: self.z_index,
        }
    }
}

#[derive(Clone, Debug)]
struct DrawMesh {
    vertex_buffer: miniquad::BufferId,
    index_buffer: miniquad::BufferId,
    texture: Option<miniquad::TextureId>,
    num_elements: i32,
}

#[derive(Debug, Clone)]
enum DrawInstruction {
    ApplyViewport(Rectangle),
    ApplyMatrix(glam::Mat4),
    Draw(DrawMesh),
}

#[derive(Debug)]
struct DrawCall {
    instrs: Vec<DrawInstruction>,
    dcs: Vec<u64>,
    z_index: u32,
}

struct RenderContext<'a> {
    ctx: &'a mut Box<dyn RenderingBackend>,
    draw_calls: &'a HashMap<u64, DrawCall>,
    uniforms_data: [u8; 128],
    white_texture: miniquad::TextureId,
}

impl<'a> RenderContext<'a> {
    fn draw(&mut self) {
        if DEBUG_RENDER {
            debug!(target: "gfx", "RenderContext::draw()");
        }
        self.draw_call(&self.draw_calls[&0], 0);
        if DEBUG_RENDER {
            debug!(target: "gfx", "RenderContext::draw() [DONE]");
        }
    }

    fn apply_view(&mut self, view: &Rectangle) {
        let (_, screen_height) = window::screen_size();

        let view_x = view.x.round() as i32;
        let view_y = screen_height - (view.y + view.h);
        let view_y = view_y.round() as i32;
        let view_w = view.w.round() as i32;
        let view_h = view.h.round() as i32;

        self.ctx.apply_viewport(view_x, view_y, view_w, view_h);
        self.ctx.apply_scissor_rect(view_x, view_y, view_w, view_h);
    }

    fn draw_call(&mut self, draw_call: &DrawCall, indent: u32) {
        let ws = if DEBUG_RENDER { " ".repeat(indent as usize * 4) } else { String::new() };

        let mut prev_view = None;

        for instr in &draw_call.instrs {
            match instr {
                DrawInstruction::ApplyViewport(view) => {
                    if DEBUG_RENDER {
                        debug!(target: "gfx", "{}apply_viewport({:?})", ws, view);
                    }
                    prev_view = Some(view.clone());
                    self.apply_view(view);
                }
                DrawInstruction::ApplyMatrix(model) => {
                    if DEBUG_RENDER {
                        debug!(target: "gfx", "{}apply_matrix(", ws);
                        debug!(target: "gfx", "{}    {:?}", ws, model.row(0).to_array());
                        debug!(target: "gfx", "{}    {:?}", ws, model.row(1).to_array());
                        debug!(target: "gfx", "{}    {:?}", ws, model.row(2).to_array());
                        debug!(target: "gfx", "{}    {:?}", ws, model.row(3).to_array());
                        debug!(target: "gfx", "{})", ws);
                    }
                    let data: [u8; 64] = unsafe { std::mem::transmute_copy(model) };
                    self.uniforms_data[64..].copy_from_slice(&data);
                    self.ctx.apply_uniforms_from_bytes(
                        self.uniforms_data.as_ptr(),
                        self.uniforms_data.len(),
                    );
                }
                DrawInstruction::Draw(mesh) => {
                    if DEBUG_RENDER {
                        debug!(target: "gfx", "{}draw({:?})", ws, mesh);
                    }
                    let texture = match mesh.texture {
                        Some(texture) => texture,
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
            }
        }

        let mut draw_calls: Vec<_> =
            draw_call.dcs.iter().map(|key| &self.draw_calls[key]).collect();
        draw_calls.sort_unstable_by_key(|dc| dc.z_index);

        for dc in draw_calls {
            self.draw_call(dc, indent + 1);

            // Reset view back again in case the draw call changed it
            if let Some(view) = &prev_view {
                if DEBUG_RENDER {
                    debug!(target: "gfx", "{}reset viewport to {:?}", ws, view);
                }
                self.apply_view(view);
            }
        }
    }
}

#[derive(Debug)]
pub enum GraphicsMethod {
    NewTexture((u16, u16, Vec<u8>, GfxTextureId)),
    DeleteTexture(GfxTextureId),
    NewVertexBuffer((Vec<Vertex>, GfxBufferId)),
    NewIndexBuffer((Vec<u16>, GfxBufferId)),
    DeleteBuffer(GfxBufferId),
    ReplaceDrawCalls(Vec<(u64, GfxDrawCall)>),
}

pub type GraphicsEventPublisherPtr = Arc<GraphicsEventPublisher>;

pub struct GraphicsEventPublisher {
    lock_resize: SyncMutex<Option<SubscriptionId>>,
    resize: PublisherPtr<(f32, f32)>,

    lock_mouse_move: SyncMutex<Option<SubscriptionId>>,
    mouse_move: PublisherPtr<(f32, f32)>,

    lock_mouse_wheel: SyncMutex<Option<SubscriptionId>>,
    mouse_wheel: PublisherPtr<(f32, f32)>,

    lock_mouse_btn_down: SyncMutex<Option<SubscriptionId>>,
    mouse_btn_down: PublisherPtr<(MouseButton, f32, f32)>,

    lock_mouse_btn_up: SyncMutex<Option<SubscriptionId>>,
    mouse_btn_up: PublisherPtr<(MouseButton, f32, f32)>,

    lock_char: SyncMutex<Option<SubscriptionId>>,
    chr: PublisherPtr<(char, KeyMods, bool)>,

    lock_key_down: SyncMutex<Option<SubscriptionId>>,
    key_down: PublisherPtr<(KeyCode, KeyMods, bool)>,

    lock_key_up: SyncMutex<Option<SubscriptionId>>,
    key_up: PublisherPtr<(KeyCode, KeyMods)>,

    lock_touch: SyncMutex<Option<SubscriptionId>>,
    touch: PublisherPtr<(TouchPhase, u64, f32, f32)>,
}

impl GraphicsEventPublisher {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            lock_resize: SyncMutex::new(None),
            resize: Publisher::new(),

            lock_mouse_move: SyncMutex::new(None),
            mouse_move: Publisher::new(),

            lock_mouse_wheel: SyncMutex::new(None),
            mouse_wheel: Publisher::new(),

            lock_mouse_btn_down: SyncMutex::new(None),
            mouse_btn_down: Publisher::new(),

            lock_mouse_btn_up: SyncMutex::new(None),
            mouse_btn_up: Publisher::new(),

            lock_char: SyncMutex::new(None),
            chr: Publisher::new(),

            lock_key_down: SyncMutex::new(None),
            key_down: Publisher::new(),

            lock_key_up: SyncMutex::new(None),
            key_up: Publisher::new(),

            lock_touch: SyncMutex::new(None),
            touch: Publisher::new(),
        })
    }

    /*
    fn lock_resize(&self, sub_id: SubscriptionId) {
        *self.lock_resize.lock().unwrap() = Some(sub_id);
    }
    fn unlock_resize(&self) {
        *self.lock_resize.lock().unwrap() = None;
    }

    fn lock_mouse_move(&self, sub_id: SubscriptionId) {
        *self.lock_mouse_move.lock().unwrap() = Some(sub_id);
    }
    fn unlock_mouse_move(&self) {
        *self.lock_mouse_move.lock().unwrap() = None;
    }

    fn lock_mouse_wheel(&self, sub_id: SubscriptionId) {
        *self.lock_mouse_wheel.lock().unwrap() = Some(sub_id);
    }
    fn unlock_mouse_wheel(&self) {
        *self.lock_mouse_wheel.lock().unwrap() = None;
    }

    fn lock_mouse_btn_down(&self, sub_id: SubscriptionId) {
        *self.lock_mouse_btn_down.lock().unwrap() = Some(sub_id);
    }
    fn unlock_mouse_btn_down(&self) {
        *self.lock_mouse_btn_down.lock().unwrap() = None;
    }

    fn lock_mouse_btn_up(&self, sub_id: SubscriptionId) {
        *self.lock_mouse_btn_up.lock().unwrap() = Some(sub_id);
    }
    fn unlock_mouse_btn_up(&self) {
        *self.lock_mouse_btn_up.lock().unwrap() = None;
    }

    fn lock_char(&self, sub_id: SubscriptionId) {
        *self.lock_char.lock().unwrap() = Some(sub_id);
    }
    fn unlock_char(&self) {
        *self.lock_char.lock().unwrap() = None;
    }

    fn lock_key_down(&self, sub_id: SubscriptionId) {
        *self.lock_key_down.lock().unwrap() = Some(sub_id);
    }
    fn unlock_key_down(&self) {
        *self.lock_key_down.lock().unwrap() = None;
    }

    fn lock_key_up(&self, sub_id: SubscriptionId) {
        *self.lock_key_up.lock().unwrap() = Some(sub_id);
    }
    fn unlock_key_up(&self) {
        *self.lock_key_up.lock().unwrap() = None;
    }

    fn lock_touch(&self, sub_id: SubscriptionId) {
        *self.lock_touch.lock().unwrap() = Some(sub_id);
    }
    fn unlock_touch(&self) {
        *self.lock_touch.lock().unwrap() = None;
    }
    */

    fn notify_resize(&self, w: f32, h: f32) {
        let ev = (w, h);

        let locked = self.lock_resize.lock().unwrap().clone();
        if let Some(locked) = locked {
            self.resize.notify_with_include(ev, &[locked]);
        } else {
            self.resize.notify(ev);
        }
    }

    fn notify_mouse_move(&self, x: f32, y: f32) {
        let ev = (x, y);

        let locked = self.lock_mouse_move.lock().unwrap().clone();
        if let Some(locked) = locked {
            self.mouse_move.notify_with_include(ev, &[locked]);
        } else {
            self.mouse_move.notify(ev);
        }
    }
    fn notify_mouse_wheel(&self, x: f32, y: f32) {
        let ev = (x, y);

        let locked = self.lock_mouse_wheel.lock().unwrap().clone();
        if let Some(locked) = locked {
            self.mouse_wheel.notify_with_include(ev, &[locked]);
        } else {
            self.mouse_wheel.notify(ev);
        }
    }
    fn notify_mouse_btn_down(&self, button: MouseButton, x: f32, y: f32) {
        let ev = (button, x, y);

        let locked = self.lock_mouse_btn_down.lock().unwrap().clone();
        if let Some(locked) = locked {
            self.mouse_btn_down.notify_with_include(ev, &[locked]);
        } else {
            self.mouse_btn_down.notify(ev);
        }
    }
    fn notify_mouse_btn_up(&self, button: MouseButton, x: f32, y: f32) {
        let ev = (button, x, y);

        let locked = self.lock_mouse_btn_up.lock().unwrap().clone();
        if let Some(locked) = locked {
            self.mouse_btn_up.notify_with_include(ev, &[locked]);
        } else {
            self.mouse_btn_up.notify(ev);
        }
    }

    fn notify_char(&self, chr: char, mods: KeyMods, repeat: bool) {
        let ev = (chr, mods, repeat);

        let locked = self.lock_char.lock().unwrap().clone();
        if let Some(locked) = locked {
            self.chr.notify_with_include(ev, &[locked]);
        } else {
            self.chr.notify(ev);
        }
    }

    fn notify_key_down(&self, key: KeyCode, mods: KeyMods, repeat: bool) {
        let ev = (key, mods, repeat);

        let locked = self.lock_key_down.lock().unwrap().clone();
        if let Some(locked) = locked {
            self.key_down.notify_with_include(ev, &[locked]);
        } else {
            self.key_down.notify(ev);
        }
    }
    fn notify_key_up(&self, key: KeyCode, mods: KeyMods) {
        let ev = (key, mods);

        let locked = self.lock_key_up.lock().unwrap().clone();
        if let Some(locked) = locked {
            self.key_up.notify_with_include(ev, &[locked]);
        } else {
            self.key_up.notify(ev);
        }
    }

    fn notify_touch(&self, phase: TouchPhase, id: u64, x: f32, y: f32) {
        let ev = (phase, id, x, y);

        let locked = self.lock_touch.lock().unwrap().clone();
        if let Some(locked) = locked {
            self.touch.notify_with_include(ev, &[locked]);
        } else {
            self.touch.notify(ev);
        }
    }

    pub fn subscribe_resize(&self) -> Subscription<(f32, f32)> {
        self.resize.clone().subscribe()
    }
    pub fn subscribe_mouse_move(&self) -> Subscription<(f32, f32)> {
        self.mouse_move.clone().subscribe()
    }
    pub fn subscribe_mouse_wheel(&self) -> Subscription<(f32, f32)> {
        self.mouse_wheel.clone().subscribe()
    }
    pub fn subscribe_mouse_btn_down(&self) -> Subscription<(MouseButton, f32, f32)> {
        self.mouse_btn_down.clone().subscribe()
    }
    pub fn subscribe_mouse_btn_up(&self) -> Subscription<(MouseButton, f32, f32)> {
        self.mouse_btn_up.clone().subscribe()
    }
    pub fn subscribe_char(&self) -> Subscription<(char, KeyMods, bool)> {
        self.chr.clone().subscribe()
    }
    pub fn subscribe_key_down(&self) -> Subscription<(KeyCode, KeyMods, bool)> {
        self.key_down.clone().subscribe()
    }
    pub fn subscribe_key_up(&self) -> Subscription<(KeyCode, KeyMods)> {
        self.key_up.clone().subscribe()
    }
    pub fn subscribe_touch(&self) -> Subscription<(TouchPhase, u64, f32, f32)> {
        self.touch.clone().subscribe()
    }
}

struct Stage {
    #[allow(dead_code)]
    app: AppPtr,
    #[allow(dead_code)]
    async_runtime: AsyncRuntime,

    ctx: Box<dyn RenderingBackend>,
    pipeline: Pipeline,
    white_texture: miniquad::TextureId,
    draw_calls: HashMap<u64, DrawCall>,
    last_draw_time: Option<Instant>,

    textures: HashMap<GfxTextureId, miniquad::TextureId>,
    buffers: HashMap<GfxBufferId, miniquad::BufferId>,

    method_rep: mpsc::Receiver<GraphicsMethod>,
    event_pub: GraphicsEventPublisherPtr,
}

impl Stage {
    pub fn new(
        app: AppPtr,
        async_runtime: AsyncRuntime,
        method_rep: mpsc::Receiver<GraphicsMethod>,
        event_pub: GraphicsEventPublisherPtr,
    ) -> Self {
        let mut ctx: Box<dyn RenderingBackend> = window::new_rendering_backend();

        // Maybe should be patched upstream since inconsistent behaviour
        // Needs testing on other platforms too.
        #[cfg(target_os = "android")]
        {
            let (screen_width, screen_height) = window::screen_size();
            event_pub.notify_resize(screen_width, screen_height);
        }

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
            app,
            async_runtime,
            ctx,
            pipeline,
            white_texture,
            draw_calls: HashMap::from([(0, DrawCall { instrs: vec![], dcs: vec![], z_index: 0 })]),
            last_draw_time: None,
            textures: HashMap::new(),
            buffers: HashMap::new(),
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
            GraphicsMethod::ReplaceDrawCalls(dcs) => self.method_replace_draw_calls(dcs),
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
        self.textures.insert(gfx_texture_id, texture);
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
        self.buffers.insert(gfx_buffer_id, buffer);
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
        self.buffers.insert(gfx_buffer_id, buffer);
    }
    fn method_delete_buffer(&mut self, gfx_buffer_id: GfxBufferId) {
        let buffer = self.buffers.remove(&gfx_buffer_id).expect("couldn't find gfx_buffer_id");
        if DEBUG_GFXAPI {
            debug!(target: "gfx", "Invoked method: delete_buffer({} => {:?})",
                   gfx_buffer_id, buffer);
        }
        self.ctx.delete_buffer(buffer);
    }
    fn method_replace_draw_calls(&mut self, dcs: Vec<(u64, GfxDrawCall)>) {
        if DEBUG_GFXAPI {
            debug!(target: "gfx", "Invoked method: replace_draw_calls({:?})", dcs);
        }
        for (key, val) in dcs {
            let val = val.compile(&self.textures, &self.buffers);
            self.draw_calls.insert(key, val);
        }
    }
}

impl EventHandler for Stage {
    fn update(&mut self) {
        // Process as many methods as we can
        while let Ok(method) = self.method_rep.try_recv() {
            self.process_method(method);
        }
    }

    fn draw(&mut self) {
        self.last_draw_time = Some(Instant::now());

        self.ctx.begin_default_pass(PassAction::Nothing);
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

        let mut render_ctx = RenderContext {
            ctx: &mut self.ctx,
            draw_calls: &self.draw_calls,
            uniforms_data,
            white_texture: self.white_texture,
        };
        render_ctx.draw();

        self.ctx.commit_frame();
    }

    fn resize_event(&mut self, width: f32, height: f32) {
        self.event_pub.notify_resize(width, height);
    }

    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        self.event_pub.notify_mouse_move(x, y);
    }
    fn mouse_wheel_event(&mut self, x: f32, y: f32) {
        self.event_pub.notify_mouse_wheel(x, y);
    }
    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        self.event_pub.notify_mouse_btn_down(button, x, y);
    }
    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        self.event_pub.notify_mouse_btn_up(button, x, y);
    }

    fn char_event(&mut self, chr: char, mods: KeyMods, repeat: bool) {
        self.event_pub.notify_char(chr, mods, repeat);
    }

    fn key_down_event(&mut self, keycode: KeyCode, mods: KeyMods, repeat: bool) {
        self.event_pub.notify_key_down(keycode, mods, repeat);
    }
    fn key_up_event(&mut self, keycode: KeyCode, mods: KeyMods) {
        self.event_pub.notify_key_up(keycode, mods);
    }

    /// The id corresponds to multi-touch. Multiple touch events have different ids.
    fn touch_event(&mut self, phase: TouchPhase, id: u64, x: f32, y: f32) {
        self.event_pub.notify_touch(phase, id, x, y);
    }

    fn quit_requested_event(&mut self) {
        debug!(target: "gfx", "quit requested");
        // Doesn't work
        //miniquad::window::cancel_quit();
        //self.app.stop();
        //self.async_runtime.stop();
    }
}

pub fn run_gui(
    app: AppPtr,
    async_runtime: AsyncRuntime,
    method_rep: mpsc::Receiver<GraphicsMethod>,
    event_pub: GraphicsEventPublisherPtr,
) {
    let mut conf = miniquad::conf::Conf {
        high_dpi: true,
        window_resizable: true,
        platform: miniquad::conf::Platform {
            linux_backend: miniquad::conf::LinuxBackend::WaylandWithX11Fallback,
            wayland_use_fallback_decorations: false,
            //blocking_event_loop: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let metal = std::env::args().nth(1).as_deref() == Some("metal");
    conf.platform.apple_gfx_api =
        if metal { conf::AppleGfxApi::Metal } else { conf::AppleGfxApi::OpenGl };

    miniquad::start(conf, || Box::new(Stage::new(app, async_runtime, method_rep, event_pub)));
}
