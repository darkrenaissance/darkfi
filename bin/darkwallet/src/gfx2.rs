use darkfi_serial::{SerialDecodable, SerialEncodable};
use log::debug;
use miniquad::{
    conf, window, Backend, Bindings, BlendFactor, BlendState, BlendValue, BufferId, BufferLayout,
    BufferSource, BufferType, BufferUsage, Equation, EventHandler, KeyCode, KeyMods, PassAction,
    Pipeline, PipelineParams, RenderingBackend, ShaderMeta, ShaderSource, TextureId, UniformDesc,
    UniformType, VertexAttribute, VertexFormat,
};
use std::{
    collections::HashMap,
    sync::{mpsc, Arc, Mutex as SyncMutex},
    time::{Duration, Instant},
};

use crate::{
    app::AsyncRuntime,
    error::{Error, Result},
    pubsub::{Publisher, PublisherPtr, Subscription, SubscriptionId},
    shader,
    util::ansi_texture,
};

// This is very noisy so suppress output by default
const DEBUG_RENDER: bool = false;

#[derive(Debug, SerialEncodable, SerialDecodable)]
#[repr(C)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn unpack(&self) -> (f32, f32) {
        (self.x, self.y)
    }

    pub fn offset(&self, off_x: f32, off_y: f32) -> Self {
        Self { x: self.x + off_x, y: self.y + off_y }
    }

    pub fn to_rect(&self, w: f32, h: f32) -> Rectangle {
        Rectangle { x: self.x, y: self.y, w, h }
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

    pub fn contains(&self, point: &Point) -> bool {
        self.x <= point.x &&
            point.x <= self.x + self.w &&
            self.y <= point.y &&
            point.y <= self.y + self.h
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

    pub async fn new_texture(&self, width: u16, height: u16, data: Vec<u8>) -> Result<TextureId> {
        let (sendr, recvr) = async_channel::bounded(1);

        let method = GraphicsMethod::NewTexture((width, height, data, sendr));

        self.method_req.send(method).map_err(|_| Error::GfxWindowClosed)?;

        let texture_id = recvr.recv().await.map_err(|_| Error::GfxWindowClosed)?;
        Ok(texture_id)
    }

    pub fn delete_texture(&self, texture: TextureId) {
        let method = GraphicsMethod::DeleteTexture(texture);

        // Ignore any error
        let _ = self.method_req.send(method);
    }

    pub async fn new_vertex_buffer(&self, verts: Vec<Vertex>) -> Result<BufferId> {
        let (sendr, recvr) = async_channel::bounded(1);

        let method = GraphicsMethod::NewVertexBuffer((verts, sendr));

        self.method_req.send(method).map_err(|_| Error::GfxWindowClosed)?;

        let buffer = recvr.recv().await.map_err(|_| Error::GfxWindowClosed)?;
        Ok(buffer)
    }

    pub async fn new_index_buffer(&self, indices: Vec<u16>) -> Result<BufferId> {
        let (sendr, recvr) = async_channel::bounded(1);

        let method = GraphicsMethod::NewIndexBuffer((indices, sendr));

        self.method_req.send(method).map_err(|_| Error::GfxWindowClosed)?;

        let buffer = recvr.recv().await.map_err(|_| Error::GfxWindowClosed)?;
        Ok(buffer)
    }

    pub fn delete_buffer(&self, buffer: BufferId) {
        let method = GraphicsMethod::DeleteBuffer(buffer);

        // Ignore any error
        let _ = self.method_req.send(method);
    }

    pub async fn replace_draw_calls(&self, dcs: Vec<(u64, DrawCall)>) {
        let method = GraphicsMethod::ReplaceDrawCalls(dcs);

        // Ignore any error
        let _ = self.method_req.send(method);
    }
}

#[derive(Clone, Debug)]
pub struct DrawMesh {
    pub vertex_buffer: BufferId,
    pub index_buffer: BufferId,
    pub texture: Option<TextureId>,
    pub num_elements: i32,
}

#[derive(Debug)]
pub enum DrawInstruction {
    ApplyViewport(Rectangle),
    ApplyMatrix(glam::Mat4),
    Draw(DrawMesh),
}

#[derive(Debug)]
pub struct DrawCall {
    pub instrs: Vec<DrawInstruction>,
    pub dcs: Vec<u64>,
    pub z_index: u32,
}

struct RenderContext<'a> {
    ctx: &'a mut Box<dyn RenderingBackend>,
    draw_calls: &'a HashMap<u64, DrawCall>,
    uniforms_data: [u8; 128],
    white_texture: TextureId,
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

    fn draw_call(&mut self, draw_call: &DrawCall, indent: u32) {
        let ws = if DEBUG_RENDER { " ".repeat(indent as usize * 4) } else { String::new() };

        for instr in &draw_call.instrs {
            match instr {
                DrawInstruction::ApplyViewport(view) => {
                    if DEBUG_RENDER {
                        debug!(target: "gfx", "{}apply_viewport({:?})", ws, view);
                    }
                    let (_, screen_height) = window::screen_size();

                    let view_x = view.x.round() as i32;
                    let view_y = screen_height - (view.y + view.h);
                    let view_y = view_y.round() as i32;
                    let view_w = view.w.round() as i32;
                    let view_h = view.h.round() as i32;

                    self.ctx.apply_viewport(view_x, view_y, view_w, view_h);
                    self.ctx.apply_scissor_rect(view_x, view_y, view_w, view_h);
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
        }
    }
}

#[derive(Debug)]
pub enum GraphicsMethod {
    NewTexture((u16, u16, Vec<u8>, async_channel::Sender<TextureId>)),
    DeleteTexture(TextureId),
    NewVertexBuffer((Vec<Vertex>, async_channel::Sender<BufferId>)),
    NewIndexBuffer((Vec<u16>, async_channel::Sender<BufferId>)),
    DeleteBuffer(BufferId),
    ReplaceDrawCalls(Vec<(u64, DrawCall)>),
}

pub type GraphicsEventPublisherPtr = Arc<GraphicsEventPublisher>;

pub struct GraphicsEventPublisher {
    lock_key_down: SyncMutex<Option<SubscriptionId>>,
    key_down: PublisherPtr<(KeyCode, KeyMods, bool)>,

    resize: PublisherPtr<(f32, f32)>,
}

impl GraphicsEventPublisher {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            lock_key_down: SyncMutex::new(None),
            key_down: Publisher::new(),
            resize: Publisher::new() })
    }

    fn lock_key_down(&self, sub_id: SubscriptionId) {
        *self.lock_key_down.lock().unwrap() = Some(sub_id);
    }
    fn unlock_key_down(&self) {
        *self.lock_key_down.lock().unwrap() = None;
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
    fn notify_resize(&self, w: f32, h: f32) {
        self.resize.notify((w, h));
    }

    pub fn subscribe_key_down(&self) -> Subscription<(KeyCode, KeyMods, bool)> {
        self.key_down.clone().subscribe()
    }
    pub fn subscribe_resize(&self) -> Subscription<(f32, f32)> {
        self.resize.clone().subscribe()
    }
}

struct Stage {
    async_runtime: AsyncRuntime,

    ctx: Box<dyn RenderingBackend>,
    pipeline: Pipeline,
    white_texture: TextureId,
    draw_calls: HashMap<u64, DrawCall>,
    last_draw_time: Option<Instant>,

    method_rep: mpsc::Receiver<GraphicsMethod>,
    event_pub: GraphicsEventPublisherPtr,
}

impl Stage {
    pub fn new(
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
            async_runtime,
            ctx,
            pipeline,
            white_texture,
            draw_calls: HashMap::from([(0, DrawCall { instrs: vec![], dcs: vec![], z_index: 0 })]),
            last_draw_time: None,
            method_rep,
            event_pub,
        }
    }

    fn method_new_texture(
        &mut self,
        width: u16,
        height: u16,
        data: Vec<u8>,
        sendr: async_channel::Sender<TextureId>,
    ) {
        let texture = self.ctx.new_texture_from_rgba8(width, height, &data);
        debug!(target: "gfx2", "Invoked method: new_texture({}, {}, ...) -> {:?}",
               width, height, texture);
        //debug!(target: "gfx2", "Invoked method: new_texture({}, {}, ...) -> {:?}\n{}",
        //       width, height, texture,
        //       ansi_texture(width as usize, height as usize, &data));
        sendr.try_send(texture).unwrap();
    }
    fn method_delete_texture(&mut self, texture: TextureId) {
        debug!(target: "gfx2", "Invoked method: delete_texture({:?})", texture);
        self.ctx.delete_texture(texture);
    }
    fn method_new_vertex_buffer(
        &mut self,
        verts: Vec<Vertex>,
        sendr: async_channel::Sender<BufferId>,
    ) {
        let buffer = self.ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&verts),
        );
        debug!(target: "gfx2", "Invoked method: new_vertex_buffer({:?}) -> {:?}", verts, buffer);
        sendr.try_send(buffer).unwrap();
    }
    fn method_new_index_buffer(
        &mut self,
        indices: Vec<u16>,
        sendr: async_channel::Sender<BufferId>,
    ) {
        let buffer = self.ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&indices),
        );
        debug!(target: "gfx2", "Invoked method: new_index_buffer({:?}) -> {:?}", indices, buffer);
        sendr.try_send(buffer).unwrap();
    }
    fn method_delete_buffer(&mut self, buffer: BufferId) {
        debug!(target: "gfx2", "Invoked method: delete_buffer({:?})", buffer);
        self.ctx.delete_buffer(buffer);
    }
    fn method_replace_draw_calls(&mut self, dcs: Vec<(u64, DrawCall)>) {
        debug!(target: "gfx2", "Invoked method: replace_draw_calls({:?})", dcs);
        for (key, val) in dcs {
            self.draw_calls.insert(key, val);
        }
    }
}

impl EventHandler for Stage {
    fn update(&mut self) {
        if self.last_draw_time.is_none() {
            return
        }

        // Only allow 20 ms, process as much as we can during that time
        let elapsed_since_draw = self.last_draw_time.unwrap().elapsed();
        // We're long overdue a redraw. Exit for now
        if elapsed_since_draw > Duration::from_millis(20) {
            return
        }
        // The next redraw must happen 20ms since its last one.
        // Calculate how much time is remaining until then.
        let allowed_time = Duration::from_millis(20) - elapsed_since_draw;
        let deadline = Instant::now() + allowed_time;

        loop {
            let Ok(method) = self.method_rep.recv_deadline(deadline) else { break };
            //debug!(target: "gfx", "Received method: {:?}", method);
            match method {
                GraphicsMethod::NewTexture((width, height, data, sendr)) => {
                    self.method_new_texture(width, height, data, sendr)
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

    fn key_down_event(&mut self, keycode: KeyCode, mods: KeyMods, repeat: bool) {
        self.event_pub.notify_key_down(keycode, mods, repeat);
    }
    fn resize_event(&mut self, width: f32, height: f32) {
        self.event_pub.notify_resize(width, height);
    }

    fn quit_requested_event(&mut self) {
        self.async_runtime.stop();
    }
}

pub fn run_gui(
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
            ..Default::default()
        },
        ..Default::default()
    };
    let metal = std::env::args().nth(1).as_deref() == Some("metal");
    conf.platform.apple_gfx_api =
        if metal { conf::AppleGfxApi::Metal } else { conf::AppleGfxApi::OpenGl };

    miniquad::start(conf, || Box::new(Stage::new(async_runtime, method_rep, event_pub)));
}
