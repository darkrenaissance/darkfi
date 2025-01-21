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

use async_trait::async_trait;
use darkfi_serial::{SerialEncodable, SerialDecodable, serialize, Encodable, Decodable, deserialize};
use std::{
    fs::{OpenOptions, File},
    collections::HashMap,
    sync::{mpsc, Arc, Mutex as SyncMutex},
    time::{Duration, Instant},
    ops::{Add, Mul},
};
use futures::AsyncWriteExt;
use miniquad::{
    conf, window, Backend, Bindings, BlendFactor, BlendState, BlendValue, BufferLayout,
    BufferSource, BufferType, BufferUsage, Equation, EventHandler, KeyCode, KeyMods, MouseButton,
    PassAction, Pipeline, PipelineParams, RenderingBackend, ShaderMeta, ShaderSource, TouchPhase,
    UniformDesc, UniformType, VertexAttribute, VertexFormat,
    UniformBlockLayout,
};

const FILENAME: &str = "drawinstrs.dat";

const DEBUG_RENDER: bool = false;
const DEBUG_GFXAPI: bool = false;

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
#[repr(C)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

#[derive(Clone, Copy, Debug, SerialEncodable, SerialDecodable)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn zero() -> Self {
        Self { x: 0., y: 0. }
    }
}

impl From<[f32; 2]> for Point {
    fn from(pos: [f32; 2]) -> Self {
        Self { x: pos[0], y: pos[1] }
    }
}

impl Add for Point {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Self { x: self.x + other.x, y: self.y + other.y }
    }
}

#[derive(Debug, Clone, Copy, SerialEncodable, SerialDecodable)]
pub struct Rectangle {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl From<[f32; 4]> for Rectangle {
    fn from(rect: [f32; 4]) -> Self {
        Self { x: rect[0], y: rect[1], w: rect[2], h: rect[3] }
    }
}

impl Mul<f32> for Rectangle {
    type Output = Rectangle;

    fn mul(self, scale: f32) -> Self::Output {
        Self { x: self.x * scale, y: self.y * scale, w: self.w * scale, h: self.h * scale }
    }
}

pub type GfxTextureId = u32;
pub type GfxBufferId = u32;

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
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

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum GfxDrawInstruction {
    SetScale(f32),
    Move(Point),
    ApplyView(Rectangle),
    Draw(GfxDrawMesh),
}

impl GfxDrawInstruction {
    fn compile(
        self,
        textures: &HashMap<GfxTextureId, miniquad::TextureId>,
        buffers: &HashMap<GfxBufferId, miniquad::BufferId>,
    ) -> DrawInstruction {
        match self {
            Self::SetScale(scale) => DrawInstruction::SetScale(scale),
            Self::Move(off) => DrawInstruction::Move(off),
            Self::ApplyView(view) => DrawInstruction::ApplyView(view),
            Self::Draw(mesh) => DrawInstruction::Draw(mesh.compile(textures, buffers)),
        }
    }
}

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
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

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub enum GraphicsMethod {
    NewTexture((u16, u16, Vec<u8>, GfxTextureId)),
    DeleteTexture(GfxTextureId),
    NewVertexBuffer((Vec<Vertex>, GfxBufferId)),
    NewIndexBuffer((Vec<u16>, GfxBufferId)),
    DeleteBuffer(GfxBufferId),
    ReplaceDrawCalls(Vec<(u64, GfxDrawCall)>),
}

#[derive(Debug, SerialEncodable, SerialDecodable)]
struct Instruction {
    timest: u64,
    method: GraphicsMethod
}

pub fn read_instrs() -> Vec<Instruction> {
    let mut instrs = vec![];
    let mut f = File::open(FILENAME).unwrap();
    loop {
        let Ok(data) = Vec::<u8>::decode(&mut f) else { break };

        let instr: Instruction = deserialize(&data).unwrap();
        instrs.push(instr);
    }
    instrs
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
    SetScale(f32),
    Move(Point),
    ApplyView(Rectangle),
    Draw(DrawMesh),
}

#[derive(Debug)]
struct DrawCall {
    instrs: Vec<DrawInstruction>,
    dcs: Vec<u64>,
    z_index: u32,
}

struct Stage {
    ctx: Box<dyn RenderingBackend>,
    pipeline: Pipeline,
    white_texture: miniquad::TextureId,
    draw_calls: HashMap<u64, DrawCall>,

    textures: HashMap<GfxTextureId, miniquad::TextureId>,
    buffers: HashMap<GfxBufferId, miniquad::BufferId>,

    instant: Instant,
    instrs: Vec<Instruction>,
}

impl Stage {
    pub fn new(
    ) -> Self {

    let mut instrs = read_instrs();
    instrs.reverse();
    println!("Loaded instrs");

        let mut ctx: Box<dyn RenderingBackend> = window::new_rendering_backend();

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
            draw_calls: HashMap::from([(0, DrawCall { instrs: vec![], dcs: vec![], z_index: 0 })]),
            textures: HashMap::new(),
            buffers: HashMap::new(),
            instant: Instant::now(),
            instrs,
        }
    }

    fn process_method(&mut self, method: GraphicsMethod) {
        //println!("Received method: {:?}", method);
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
            println!("Invoked method: new_texture({}, {}, ..., {}) -> {:?}",
                   width, height, gfx_texture_id, texture);
            //println!("Invoked method: new_texture({}, {}, ..., {}) -> {:?}\n{}",
            //       width, height, gfx_texture_id, texture,
            //       ansi_texture(width as usize, height as usize, &data));
        }
        self.textures.insert(gfx_texture_id, texture);
    }
    fn method_delete_texture(&mut self, gfx_texture_id: GfxTextureId) {
        let texture = self.textures.remove(&gfx_texture_id).expect("couldn't find gfx_texture_id");
        if DEBUG_GFXAPI {
            println!("Invoked method: delete_texture({} => {:?})",
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
            println!("Invoked method: new_vertex_buffer(..., {}) -> {:?}",
                   gfx_buffer_id, buffer);
            //println!("Invoked method: new_vertex_buffer({:?}, {}) -> {:?}",
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
            println!("Invoked method: new_index_buffer({}) -> {:?}",
                   gfx_buffer_id, buffer);
            //println!("Invoked method: new_index_buffer({:?}, {}) -> {:?}",
            //       indices, gfx_buffer_id, buffer);
        }
        self.buffers.insert(gfx_buffer_id, buffer);
    }
    fn method_delete_buffer(&mut self, gfx_buffer_id: GfxBufferId) {
        let buffer = self.buffers.remove(&gfx_buffer_id).expect("couldn't find gfx_buffer_id");
        if DEBUG_GFXAPI {
            println!("Invoked method: delete_buffer({} => {:?})",
                   gfx_buffer_id, buffer);
        }
        self.ctx.delete_buffer(buffer);
    }
    fn method_replace_draw_calls(&mut self, dcs: Vec<(u64, GfxDrawCall)>) {
        if DEBUG_GFXAPI {
            println!("Invoked method: replace_draw_calls({:?})", dcs);
        }
        for (key, val) in dcs {
            let val = val.compile(&self.textures, &self.buffers);
            self.draw_calls.insert(key, val);
        }
    }
}

impl EventHandler for Stage {
    fn update(&mut self) {
        let timest = self.instant.elapsed().as_millis() as u64;
        while let Some(instr) = self.instrs.last() {
            if instr.timest > timest {
                break
            }

            let instr = self.instrs.pop().unwrap();
            self.process_method(instr.method);
        }
    }

    fn draw(&mut self) {
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
            println!("RenderContext::draw()");
        }
        let curr_pos = Point::zero();
        self.draw_call(&self.draw_calls[&0], 0);
        if DEBUG_RENDER {
            println!("RenderContext::draw() [DONE]");
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

        //if DEBUG_RENDER {
        //    println!("=> viewport {view_x} {view_y} {view_w} {view_h}");
        //}
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

    fn draw_call(&mut self, draw_call: &DrawCall, indent: u32) {
        let ws = if DEBUG_RENDER { " ".repeat(indent as usize * 4) } else { String::new() };

        let old_view = self.view;
        let old_cursor = self.cursor;

        for instr in &draw_call.instrs {
            match instr {
                DrawInstruction::SetScale(scale) => {
                    self.scale = *scale;
                    if DEBUG_RENDER {
                        println!("{ws}set_scale({scale})");
                    }
                }
                DrawInstruction::Move(off) => {
                    self.cursor = old_cursor + *off;
                    if DEBUG_RENDER {
                        println!(
                            "{ws}move({off:?})  cursor={:?}, scale={}, view={:?}",
                            self.cursor, self.scale, self.view
                        );
                    }
                    self.apply_model();
                }
                DrawInstruction::ApplyView(view) => {
                    self.view = *view;
                    if DEBUG_RENDER {
                        println!(
                            "{ws}apply_view({view:?})  scale={}, view={:?}",
                            self.scale, self.view
                        );
                    }
                    self.apply_view();
                }
                DrawInstruction::Draw(mesh) => {
                    if DEBUG_RENDER {
                        println!("{ws}draw({mesh:?})");
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
            draw_call.dcs.iter().map(|key| (key, &self.draw_calls[key])).collect();
        draw_calls.sort_unstable_by_key(|(_, dc)| dc.z_index);

        for (dc_key, dc) in draw_calls {
            if DEBUG_RENDER {
                println!("{ws}drawcall {dc_key}");
            }
            self.draw_call(dc, indent + 1);
        }

        self.cursor = old_cursor;
        self.apply_model();

        self.view = old_view;
        self.apply_view();
    }
}

fn main() {
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

    miniquad::start(conf, || Box::new(Stage::new()));
}

mod shader {
    use super::*;

    pub const GL_VERTEX: &str = r#"#version 100
    attribute vec2 in_pos;
    attribute vec4 in_color;
    attribute vec2 in_uv;

    varying lowp vec4 color;
    varying lowp vec2 uv;

    uniform mat4 Projection;
    uniform mat4 Model;

    void main() {
        gl_Position = Projection * Model * vec4(in_pos, 0, 1);
        color = in_color;
        uv = in_uv;
    }"#;

    pub const GL_FRAGMENT: &str = r#"#version 100
    varying lowp vec4 color;
    varying lowp vec2 uv;

    uniform sampler2D tex;

    void main() {
        gl_FragColor = color * texture2D(tex, uv);
    }"#;

    pub const METAL: &str = r#"
    #include <metal_stdlib>

    using namespace metal;

    struct Uniforms
    {
        float4x4 Projection;
        float4x4 Model;
    };

    struct Vertex
    {
        float2 in_pos   [[attribute(0)]];
        float4 in_color [[attribute(1)]];
        float2 in_uv    [[attribute(2)]];
    };

    struct RasterizerData
    {
        float4 position [[position]];
        float4 color [[user(locn0)]];
        float2 uv [[user(locn1)]];
    };

    vertex RasterizerData vertexShader(Vertex v [[stage_in]])
    {
        RasterizerData out;

        out.position = uniforms.Model * uniforms.Projection * float4(v.in_pos.xy, 0.0, 1.0);
        out.color = v.in_color;
        out.uv = v.texcoord;

        return out;
    }

    fragment float4 fragmentShader(RasterizerData in [[stage_in]], texture2d<float> tex [[texture(0)]], sampler texSmplr [[sampler(0)]])
    {
        return in.color * tex.sample(texSmplr, in.uv);
    }

    "#;

    pub fn meta() -> ShaderMeta {
        ShaderMeta {
            images: vec!["tex".to_string()],
            uniforms: UniformBlockLayout { uniforms: vec![] },
        }
    }
}
